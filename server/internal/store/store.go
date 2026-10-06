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
		"&_pragma=synchronous(NORMAL)"
	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, err
	}
	if err := migrate(db); err != nil {
		db.Close()
		return nil, err
	}
	return db, nil
}

// migrate applies every migrations/NNNN_*.sql above the recorded version,
// each in its own transaction.
func migrate(db *sql.DB) error {
	if _, err := db.Exec(`CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)`); err != nil {
		return err
	}
	var current int
	if err := db.QueryRow(`SELECT COALESCE(MAX(version), 0) FROM schema_version`).Scan(&current); err != nil {
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
		if n <= current {
			continue
		}
		body, err := migrations.ReadFile("migrations/" + e.Name())
		if err != nil {
			return err
		}
		tx, err := db.Begin()
		if err != nil {
			return err
		}
		if _, err := tx.Exec(string(body)); err != nil {
			tx.Rollback()
			return fmt.Errorf("migration %s: %w", e.Name(), err)
		}
		if _, err := tx.Exec(`INSERT INTO schema_version (version) VALUES (?)`, n); err != nil {
			tx.Rollback()
			return err
		}
		if err := tx.Commit(); err != nil {
			return err
		}
	}
	return nil
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
