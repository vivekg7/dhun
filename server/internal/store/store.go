// Package store opens Dhun's SQLite database, applies migrations and takes
// backups. The schema is in migrations/ and explained in
// docs/plans/005_storage_and_library_model.md.
package store

import (
	"context"
	"database/sql"
	"embed"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	_ "modernc.org/sqlite"
)

//go:embed migrations/*.sql
var migrations embed.FS

// Open opens (creating if needed) the database at path and brings its schema
// up to date.
func Open(path string) (*sql.DB, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	dsn := "file:" + path +
		"?_pragma=journal_mode(WAL)" +
		"&_pragma=foreign_keys(1)" +
		"&_pragma=busy_timeout(10000)" +
		"&_pragma=synchronous(NORMAL)" +
		// Every transaction takes the write lock when it begins, so writers
		// queue for up to busy_timeout. Deferred transactions that read first
		// fail at once with SQLITE_BUSY when they try to write while another
		// writer is active. It also serialises the playlist file writes that
		// happen inside those transactions.
		"&_txlock=immediate"
	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, err
	}
	if err := migrate(db, 1<<31); err != nil {
		db.Close()
		return nil, err
	}
	return db, nil
}

// fkOff marks a migration that rebuilds a table other tables refer to.
// SQLite cannot change a column constraint in place, and dropping the old
// table breaks every reference to it, even with the checks deferred. Its
// documented procedure turns foreign keys off around the rebuild, which
// only works outside a transaction, and checks them before committing.
const fkOff = "-- foreign_keys: off\n"

// migrate applies every migrations/NNNN_*.sql above the recorded version,
// up to upTo (tests stop early), each in its own transaction, on one
// connection so a pragma set for a migration applies to it.
func migrate(db *sql.DB, upTo int) error {
	ctx := context.Background()
	conn, err := db.Conn(ctx)
	if err != nil {
		return err
	}
	defer conn.Close()
	if _, err := conn.ExecContext(ctx, `CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)`); err != nil {
		return err
	}
	var current int
	if err := conn.QueryRowContext(ctx, `SELECT COALESCE(MAX(version), 0) FROM schema_version`).Scan(&current); err != nil {
		return err
	}
	names, err := migrations.ReadDir("migrations")
	if err != nil {
		return err
	}
	sort.Slice(names, func(i, j int) bool { return names[i].Name() < names[j].Name() })
	for _, e := range names {
		var n int
		if _, err := fmt.Sscanf(e.Name(), "%04d_", &n); err != nil {
			return fmt.Errorf("migration %s: name must start with NNNN_", e.Name())
		}
		if n <= current || n > upTo {
			continue
		}
		body, err := migrations.ReadFile("migrations/" + e.Name())
		if err != nil {
			return err
		}
		if err := migrateOne(ctx, conn, n, string(body)); err != nil {
			return fmt.Errorf("migration %s: %w", e.Name(), err)
		}
	}
	return nil
}

func migrateOne(ctx context.Context, conn *sql.Conn, n int, body string) (err error) {
	if strings.HasPrefix(body, fkOff) {
		if _, err := conn.ExecContext(ctx, `PRAGMA foreign_keys = OFF`); err != nil {
			return err
		}
		defer func() {
			if _, ferr := conn.ExecContext(ctx, `PRAGMA foreign_keys = ON`); err == nil {
				err = ferr
			}
		}()
	}
	tx, err := conn.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, body); err != nil {
		return err
	}
	// With the checks off, nothing stopped the migration breaking a
	// reference; refuse to commit one that did.
	rows, err := tx.QueryContext(ctx, `PRAGMA foreign_key_check`)
	if err != nil {
		return err
	}
	broken := rows.Next()
	rows.Close()
	if broken {
		return fmt.Errorf("it leaves a foreign key pointing at nothing")
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO schema_version (version) VALUES (?)`, n); err != nil {
		return err
	}
	return tx.Commit()
}

// NextLibraryVersion bumps and returns the library version inside tx.
func NextLibraryVersion(ctx context.Context, tx *sql.Tx) (int64, error) {
	var v int64
	err := tx.QueryRowContext(ctx,
		`UPDATE meta SET value = value + 1 WHERE key = 'library_version' RETURNING value`).Scan(&v)
	return v, err
}

// Backup writes a consistent snapshot to dir/dhun-YYYY-MM-DD.db unless one
// for today exists, and keeps the newest `keep` snapshots. Copying the live
// WAL database file is not guaranteed consistent; these snapshots are what
// the NAS backup should pick up.
func Backup(ctx context.Context, db *sql.DB, dir string, keep int, now time.Time) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	target := filepath.Join(dir, "dhun-"+now.Format("2006-01-02")+".db")
	if _, err := os.Stat(target); err == nil {
		return nil
	}
	if _, err := db.ExecContext(ctx, `VACUUM INTO ?`, target); err != nil {
		return err
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return err
	}
	var snaps []string
	for _, e := range entries {
		if strings.HasPrefix(e.Name(), "dhun-") && strings.HasSuffix(e.Name(), ".db") {
			snaps = append(snaps, e.Name())
		}
	}
	sort.Strings(snaps) // dates sort lexically
	for len(snaps) > keep {
		if err := os.Remove(filepath.Join(dir, snaps[0])); err != nil {
			return err
		}
		snaps = snaps[1:]
	}
	return nil
}

// Now is the timestamp format stored in the database: UTC, RFC 3339.
func Now() string { return time.Now().UTC().Format(time.RFC3339) }
