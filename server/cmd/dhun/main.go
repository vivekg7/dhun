// Command dhun is the Dhun server.
//
//	dhun                        serve (the default)
//	dhun healthcheck            exit 0 if the local server answers /healthz
//	dhun user passwd <name>     reset a password, e.g. the admin's own
//
// Configuration is environment variables with defaults that fit the Docker
// image (docs/plans/003_deployment.md):
//
//	DHUN_MEDIA           media root, the mounted Music folder   (/media)
//	DHUN_ADDR            listen address                        (:8585)
//	DHUN_RESCAN          interval between automatic rescans    (1h)
//	DHUN_ADMIN_USER      the admin, created on first start while there are
//	DHUN_ADMIN_PASSWORD  no users; ignored after that
package main

import (
	"bufio"
	"context"
	"database/sql"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"
	_ "time/tzdata" // the image is FROM scratch: no zoneinfo, yet TZ must work

	"golang.org/x/term"

	"github.com/vivekg7/dhun/server/internal/api"
	"github.com/vivekg7/dhun/server/internal/library"
	"github.com/vivekg7/dhun/server/internal/store"
)

func main() {
	log := slog.New(slog.NewTextHandler(os.Stderr, nil))
	if err := run(log, os.Args[1:]); err != nil {
		fmt.Fprintln(os.Stderr, "dhun:", err)
		os.Exit(1)
	}
}

func env(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}

func run(log *slog.Logger, args []string) error {
	if len(args) > 0 && args[0] == "healthcheck" {
		// Docker's HEALTHCHECK: the scratch image has no curl or wget.
		addr := env("DHUN_ADDR", ":8585")
		if strings.HasPrefix(addr, ":") {
			addr = "127.0.0.1" + addr
		}
		client := http.Client{Timeout: 5 * time.Second}
		resp, err := client.Get("http://" + addr + "/healthz")
		if err != nil {
			return err
		}
		resp.Body.Close()
		if resp.StatusCode != http.StatusOK {
			return fmt.Errorf("healthz: %s", resp.Status)
		}
		return nil
	}
	media, err := filepath.Abs(env("DHUN_MEDIA", "/media"))
	if err != nil {
		return err
	}
	if info, err := os.Stat(media); err != nil || !info.IsDir() {
		return fmt.Errorf("media folder %s is not mounted (set DHUN_MEDIA)", media)
	}
	dataDir := filepath.Join(media, "_dhun")
	db, err := store.Open(filepath.Join(dataDir, "dhun.db"))
	if err != nil {
		return err
	}
	defer db.Close()

	if len(args) > 0 && args[0] == "user" {
		return userCmd(db, args[1:])
	}
	if len(args) > 0 && args[0] != "serve" {
		return fmt.Errorf("unknown command %q", args[0])
	}

	created, err := api.EnsureAdmin(context.Background(), db, os.Getenv("DHUN_ADMIN_USER"), os.Getenv("DHUN_ADMIN_PASSWORD"))
	if err != nil {
		return err
	}
	if created {
		log.Info("admin created; add family members from the admin panel", "admin", os.Getenv("DHUN_ADMIN_USER"))
	}

	interval, err := time.ParseDuration(env("DHUN_RESCAN", "1h"))
	if err != nil {
		return fmt.Errorf("DHUN_RESCAN: %w", err)
	}
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()

	scanner := &library.Scanner{DB: db, Root: media, Log: log}
	// Rescan requests while a scan runs collapse into one follow-up scan.
	trigger := make(chan struct{}, 1)
	rescan := func() {
		select {
		case trigger <- struct{}{}:
		default:
		}
	}
	go func() {
		tick := time.NewTicker(interval)
		defer tick.Stop()
		for {
			start := time.Now()
			st, err := scanner.Scan(ctx)
			if err != nil && ctx.Err() == nil {
				log.Error("scan failed", "err", err)
			} else if err == nil {
				log.Info("scan done", "took", time.Since(start).Round(time.Millisecond), "stats", st.String())
			}
			if err := store.Backup(ctx, db, filepath.Join(dataDir, "backups"), 14, time.Now()); err != nil && ctx.Err() == nil {
				log.Error("backup failed", "err", err)
			}
			select {
			case <-ctx.Done():
				return
			case <-tick.C:
			case <-trigger:
			}
		}
	}()

	srv := &http.Server{
		Addr: env("DHUN_ADDR", ":8585"),
		Handler: (&api.Server{
			DB: db, Root: media, DataDir: dataDir, Scanner: scanner, Log: log, Rescan: rescan,
		}).Handler(),
		ReadHeaderTimeout: 10 * time.Second,
	}
	go func() {
		<-ctx.Done()
		shutdown, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		srv.Shutdown(shutdown)
	}()
	log.Info("dhun listening", "addr", srv.Addr, "media", media)
	if err := srv.ListenAndServe(); !errors.Is(err, http.ErrServerClosed) {
		return err
	}
	return nil
}

// userCmd is the way back in when the admin forgets their password:
// `docker exec -it dhun /dhun user passwd <name>`. Adding users is the admin
// panel's job, and the admin itself comes from docker-compose.yml.
func userCmd(db *sql.DB, args []string) error {
	if len(args) != 2 || args[0] != "passwd" {
		return errors.New("usage: dhun user passwd <name>")
	}
	password, err := readPassword()
	if err != nil {
		return err
	}
	if err := api.SetPassword(context.Background(), db, args[1], password); err != nil {
		return err
	}
	fmt.Println("password changed; all of the user's devices are signed out")
	return nil
}

// readPassword prompts twice on a terminal, or reads one line from stdin
// (for scripting: `echo "$PW" | dhun user passwd ...`).
func readPassword() (string, error) {
	fd := int(os.Stdin.Fd())
	if !term.IsTerminal(fd) {
		line, err := bufio.NewReader(os.Stdin).ReadString('\n')
		if err != nil && line == "" {
			return "", err
		}
		return strings.TrimRight(line, "\r\n"), nil
	}
	fmt.Print("password: ")
	a, err := term.ReadPassword(fd)
	fmt.Println()
	if err != nil {
		return "", err
	}
	fmt.Print("again: ")
	b, err := term.ReadPassword(fd)
	fmt.Println()
	if err != nil {
		return "", err
	}
	if string(a) != string(b) {
		return "", errors.New("passwords do not match")
	}
	return string(a), nil
}
