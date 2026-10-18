//! `platform/payments-api`: the repository the screenshots are mostly about. It has a merge in the
//! graph, tags, a stash, a locked worktree, a dirty one, a submodule and branches that exist only
//! on the remote.

use std::fs;
use std::path::{Path, PathBuf};

use super::demo::{apply, layout, write, Git, Step, PEOPLE};

const LEDGER_V1: &str = r#"package ledger

// Entry is one movement of money on an account, in minor units.
type Entry struct {
	Account string
	Amount  int64
	Memo    string
}

// Balance sums the entries of one account.
func Balance(entries []Entry, account string) int64 {
	var total int64
	for _, e := range entries {
		if e.Account == account {
			total += e.Amount
		}
	}
	return total
}
"#;

const LEDGER_V2: &str = r#"package ledger

import "math"

// Entry is one movement of money on an account, in minor units.
type Entry struct {
	Account string
	Amount  int64
	Memo    string
}

// Balance sums the entries of one account.
func Balance(entries []Entry, account string) int64 {
	var total int64
	for _, e := range entries {
		if e.Account == account {
			total += e.Amount
		}
	}
	return total
}

// Round turns a fractional amount into minor units, half to even, so
// repeated rounding does not drift in one direction.
func Round(v float64) int64 {
	return int64(math.RoundToEven(v))
}
"#;

const LEDGER_V3: &str = r#"package ledger

import "math"

// Entry is one movement of money on an account, in minor units.
type Entry struct {
	Account string
	Amount  int64
	Memo    string
}

// Balance sums the entries of one account.
func Balance(entries []Entry, account string) int64 {
	var total int64
	for _, e := range entries {
		if e.Account == account {
			total += e.Amount
		}
	}
	return total
}

// Round turns a fractional amount into minor units, half to even, so
// repeated rounding does not drift in one direction.
func Round(v float64) int64 {
	return int64(math.RoundToEven(v))
}

// Batch collects entries and writes them in one transaction.
type Batch struct {
	entries []Entry
	limit   int
}

// Add queues an entry and reports whether the batch is full.
func (b *Batch) Add(e Entry) bool {
	b.entries = append(b.entries, e)
	return len(b.entries) >= b.limit
}
"#;

const CHARGE_V1: &str = r#"package api

import (
	"encoding/json"
	"net/http"
)

type chargeRequest struct {
	Account string `json:"account"`
	Amount  int64  `json:"amount"`
	Memo    string `json:"memo"`
}

// Charge books a charge against an account.
func Charge(w http.ResponseWriter, r *http.Request) {
	var req chargeRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "bad request", http.StatusBadRequest)
		return
	}
	w.WriteHeader(http.StatusCreated)
}
"#;

const CHARGE_V2: &str = r#"package api

import (
	"encoding/json"
	"net/http"
)

type chargeRequest struct {
	Account string `json:"account"`
	Amount  int64  `json:"amount"`
	Memo    string `json:"memo"`
}

// Charge books a charge against an account. A repeated Idempotency-Key
// returns the first answer instead of charging twice.
func Charge(w http.ResponseWriter, r *http.Request) {
	key := r.Header.Get("Idempotency-Key")
	if key == "" {
		http.Error(w, "Idempotency-Key is required", http.StatusBadRequest)
		return
	}
	if seen(key) {
		w.WriteHeader(http.StatusOK)
		return
	}
	var req chargeRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "bad request", http.StatusBadRequest)
		return
	}
	w.WriteHeader(http.StatusCreated)
}
"#;

const REFUND_V1: &str = r#"package refund

import "errors"

var ErrTooMuch = errors.New("refund is larger than the charge")

// Amount checks a full or partial refund against the original charge.
func Amount(charged, already, asked int64) (int64, error) {
	if asked <= 0 || already+asked > charged {
		return 0, ErrTooMuch
	}
	return asked, nil
}
"#;

const REFUND_V2: &str = r#"package refund

import "errors"

var (
	ErrTooMuch = errors.New("refund is larger than the charge")
	ErrTooOld  = errors.New("charge is older than the refund window")
)

// RefundWindowDays is how long after a charge a refund is accepted.
const RefundWindowDays = 120

// Amount checks a full or partial refund against the original charge.
func Amount(charged, already, asked int64, ageDays int) (int64, error) {
	if ageDays > RefundWindowDays {
		return 0, ErrTooOld
	}
	if asked <= 0 || already+asked > charged {
		return 0, ErrTooMuch
	}
	return asked, nil
}
"#;

const WEBHOOK_V1: &str = r#"package refund

import (
	"bytes"
	"net/http"
)

// Notify tells the merchant that a refund was booked.
func Notify(url string, body []byte) error {
	resp, err := http.Post(url, "application/json", bytes.NewReader(body))
	if err != nil {
		return err
	}
	return resp.Body.Close()
}
"#;

const WEBHOOK_V2: &str = r#"package refund

import (
	"bytes"
	"net/http"
	"time"
)

// Notify tells the merchant that a refund was booked and retries with a
// growing pause when the merchant answers with a server error.
func Notify(url string, body []byte) error {
	pause := 500 * time.Millisecond
	var err error
	for attempt := 0; attempt < 5; attempt++ {
		var resp *http.Response
		resp, err = http.Post(url, "application/json", bytes.NewReader(body))
		if err == nil && resp.StatusCode < 500 {
			return resp.Body.Close()
		}
		time.Sleep(pause)
		pause *= 2
	}
	return err
}
"#;

const MIGRATION: &str = r#"-- 0042: refunds keep a pointer to the charge they reverse.
CREATE TABLE refunds (
    id          BIGSERIAL PRIMARY KEY,
    charge_id   BIGINT      NOT NULL REFERENCES charges (id),
    amount      BIGINT      NOT NULL CHECK (amount > 0),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX refunds_charge_idx ON refunds (charge_id);
"#;

const CI: &str = r#"name: ci
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with:
          go-version: "1.24"
      - run: go vet ./...
      - run: go test ./...
"#;

const MAIN_GO: &str = "package main\n\nfunc main() {\n\t// wiring lives in internal/api\n}\n";

const GO_MOD_V1: &str = "module github.com/northwind-labs/payments-api\n\ngo 1.22\n";
const GO_MOD_V2: &str = "module github.com/northwind-labs/payments-api\n\ngo 1.24\n";

const WEBHOOK_DOC: &str = r#"# Webhooks

Every booked refund is posted to the merchant's URL as JSON:

    {"refund_id": 8841, "charge_id": 20917, "amount": 1250, "currency": "EUR"}

The request carries `X-Signature`, an HMAC-SHA256 of the body.
"#;

/// Early history of `main`: scaffold, ledger, charge, CI, refunds.
const EARLY: [Step; 6] = [
    Step { who: 3, days_ago: 520, message: "chore: scaffold payments-api", files: &[("go.mod", GO_MOD_V1), ("cmd/server/main.go", MAIN_GO), ("README.md", "# payments-api\n\nCharges, refunds and the ledger behind them.\n")] },
    Step { who: 2, days_ago: 506, message: "feat: ledger entries and balance", files: &[("internal/ledger/ledger.go", LEDGER_V1)] },
    Step { who: 1, days_ago: 489, message: "feat: charge endpoint", files: &[("internal/api/charge.go", CHARGE_V1)] },
    Step { who: 0, days_ago: 471, message: "ci: run vet and tests on every push", files: &[(".github/workflows/ci.yml", CI)] },
    Step { who: 4, days_ago: 452, message: "feat: refunds, full and partial", files: &[("internal/refund/refund.go", REFUND_V1)] },
    Step { who: 4, days_ago: 450, message: "db: add the refunds table", files: &[("migrations/0042_refunds.sql", MIGRATION)] },
];

const ROUNDING: [Step; 2] = [
    Step { who: 7, days_ago: 410, message: "fix: round half to even in ledger totals", files: &[("internal/ledger/ledger.go", LEDGER_V2)] },
    Step { who: 7, days_ago: 408, message: "test: cover rounding of .5 amounts", files: &[("internal/ledger/ledger_test.go", "package ledger\n\nimport \"testing\"\n\nfunc TestRoundHalfEven(t *testing.T) {\n\tif Round(2.5) != 2 || Round(3.5) != 4 {\n\t\tt.Fatal(\"half must go to even\")\n\t}\n}\n")] },
];

const REFUND_WEBHOOKS: [Step; 3] = [
    Step { who: 8, days_ago: 62, message: "feat: notify the merchant when a refund is booked", files: &[("internal/refund/webhook.go", WEBHOOK_V1)] },
    Step { who: 8, days_ago: 55, message: "feat: retry the webhook with a growing pause", files: &[("internal/refund/webhook.go", WEBHOOK_V2)] },
    Step { who: 6, days_ago: 49, message: "docs: describe the webhook signature", files: &[("docs/webhooks.md", "# Webhooks\n\nEvery booked refund is posted to the merchant's URL as JSON.\n\nThe request carries `X-Signature`, an HMAC-SHA256 of the body.\n\nRetries: five attempts, pausing 0.5 s, 1 s, 2 s, 4 s.\n")] },
];

/// Builds the repository under `work/platform/payments-api` and returns its root.
pub fn build(git: &Git, work: &Path, seeds: &Path) -> PathBuf {
    let seed = seeds.join("payments-api");
    fs::create_dir_all(&seed).expect("seed");
    git.run(&seed, &["init", "-q", "-b", "main"]);
    apply(git, &seed, &EARLY);
    let people = &PEOPLE;

    // The contracts live in their own repository and are a submodule here.
    let proto = seeds.join("proto-contracts");
    fs::create_dir_all(&proto).expect("proto");
    git.run(&proto, &["init", "-q", "-b", "main"]);
    write(&proto, "payments.proto", "syntax = \"proto3\";\n\nmessage Refund {\n  int64 id = 1;\n  int64 amount = 2;\n}\n");
    git.commit(&proto, &people[0], 480, 10, "feat: refund message");
    let proto_str = proto.to_str().expect("utf8");
    git.run(&seed, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", proto_str, "proto"]);
    git.commit(&seed, &people[0], 440, 10, "chore: add the proto-contracts submodule");
    git.tag(&seed, &people[0], 438, "v2.2.0", "Release 2.2.0");

    git.run(&seed, &["switch", "-q", "-c", "fix/rounding-error"]);
    apply(git, &seed, &ROUNDING);
    git.run(&seed, &["switch", "-q", "main"]);
    apply(git, &seed, &[Step { who: 5, days_ago: 405, message: "feat: idempotency keys on charge", files: &[("internal/api/charge.go", CHARGE_V2)] }]);
    git.merge(&seed, &people[1], 402, "fix/rounding-error", "Merge branch 'fix/rounding-error'");
    apply(git, &seed, &[Step { who: 6, days_ago: 360, message: "docs: document the webhook payload", files: &[("docs/webhooks.md", WEBHOOK_DOC)] }]);
    apply(git, &seed, &[Step { who: 0, days_ago: 340, message: "chore: release 2.3.0", files: &[("VERSION", "2.3.0\n")] }]);
    git.tag(&seed, &people[0], 340, "v2.3.0", "Release 2.3.0");

    git.run(&seed, &["switch", "-q", "-c", "feature/refund-webhooks"]);
    apply(git, &seed, &REFUND_WEBHOOKS);
    git.run(&seed, &["switch", "-q", "main"]);
    apply(git, &seed, &[Step { who: 2, days_ago: 52, message: "perf: batch ledger writes", files: &[("internal/ledger/ledger.go", LEDGER_V3)] }]);
    apply(git, &seed, &[Step { who: 3, days_ago: 30, message: "chore: build with go 1.24", files: &[("go.mod", GO_MOD_V2)] }]);
    apply(git, &seed, &[Step { who: 0, days_ago: 21, message: "chore: release 2.4.0", files: &[("VERSION", "2.4.0\n")] }]);
    git.tag(&seed, &people[0], 21, "v2.4.0", "Release 2.4.0");
    git.run(&seed, &["branch", "release/2.4"]);
    apply(git, &seed, &[Step { who: 9, days_ago: 9, message: "fix: reject refunds on charges older than 120 days", files: &[("internal/refund/refund.go", REFUND_V2)] }]);
    apply(git, &seed, &[Step { who: 10, days_ago: 4, message: "docs: add the refund limits to the readme", files: &[("README.md", "# payments-api\n\nCharges, refunds and the ledger behind them.\n\n## Limits\n\nA charge can be refunded for 120 days.\n")] }]);
    git.run(&seed, &["switch", "-q", "-c", "chore/deps"]);
    apply(git, &seed, &[Step { who: 11, days_ago: 2, message: "chore: update the golang image to 1.24.3", files: &[("Dockerfile", "FROM golang:1.24.3-alpine AS build\nWORKDIR /src\nCOPY . .\nRUN go build -o /out/server ./cmd/server\n")] }]);
    for remote_only in ["feature/batch-capture", "fix/webhook-retry", "dependabot/go-modules/golang.org-x-net-0.33.0"] {
        git.run(&seed, &["branch", remote_only, "main"]);
    }
    git.run(&seed, &["switch", "-q", "main"]);

    let root = work.join("platform/payments-api");
    let worktrees = ["main", "feature/refund-webhooks", "fix/rounding-error", "release/2.4"];
    let remote = [
        "main",
        "feature/refund-webhooks",
        "feature/batch-capture",
        "fix/webhook-retry",
        "dependabot/go-modules/golang.org-x-net-0.33.0",
    ];
    // The worktree of `main` is created from `main` itself; the bare clone checks out nothing.
    layout(git, &seed, &root, &worktrees, &remote);
    let bare = format!("--git-dir={}", root.join(".bare").display());
    for gone in ["feature/batch-capture", "fix/webhook-retry", "dependabot/go-modules/golang.org-x-net-0.33.0"] {
        git.run(&root, &[&bare, "branch", "-q", "-D", gone]);
    }
    git.run(&root, &[&bare, "worktree", "lock", "--reason", "release freeze", root.join("release/2.4").to_str().expect("utf8")]);
    root
}

/// Leaves `feature/refund-webhooks` the way a day of work leaves it: a stash, a staged file, an
/// unstaged one and a new one.
pub fn make_busy(git: &Git, root: &Path) {
    let wt = root.join("feature/refund-webhooks");
    write(&wt, "internal/refund/webhook.go", &WEBHOOK_V2.replace("time.Sleep(pause)", "// TODO: add jitter\n\t\ttime.Sleep(pause)"));
    git.run(&wt, &["stash", "push", "-q", "-m", "WIP: sign the webhook body"]);
    write(&wt, "internal/refund/webhook.go", &WEBHOOK_V2.replace("500 * time.Millisecond", "250 * time.Millisecond"));
    write(&wt, "internal/refund/sign.go", "package refund\n\nimport (\n\t\"crypto/hmac\"\n\t\"crypto/sha256\"\n\t\"encoding/hex\"\n)\n\n// Sign returns the X-Signature value for a webhook body.\nfunc Sign(secret, body []byte) string {\n\tmac := hmac.New(sha256.New, secret)\n\tmac.Write(body)\n\treturn hex.EncodeToString(mac.Sum(nil))\n}\n");
    git.run(&wt, &["add", "internal/refund/sign.go", "internal/refund/webhook.go"]);
    write(&wt, "internal/refund/refund.go", &REFUND_V1.replace("refund is larger than the charge", "refund is larger than what is left on the charge"));
    write(&wt, "internal/refund/testdata/event.json", "{\"refund_id\": 8841, \"charge_id\": 20917, \"amount\": 1250}\n");
}
