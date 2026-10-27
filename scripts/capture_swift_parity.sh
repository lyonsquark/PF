#!/bin/bash
# Records the read-only API replies of the running Swift PF app into fixtures/parity/ (T26).
# Run it with the Swift app open and holding fixtures/SampleData2.md. Never run it against PFRust.
set -euo pipefail

BASE="http://localhost:8989"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${ROOT}/fixtures/parity"
SWIFT_REPO="${ROOT}/../PFApp"
MAIN="Main%20Checking"
REWARDS="Rewards%20Card"
SCN="New%20Car%20Savings"

echo "Precondition 1: the Swift PF app must be running on port 8989 (PFRust uses the same port)."
if ! curl -sf "${BASE}/api/scenarios" > /dev/null; then
    echo "error: nothing answers on ${BASE}/api/scenarios. Start the Swift app and try again." >&2
    exit 1
fi

echo "Precondition 2: the Swift app must hold exactly fixtures/SampleData2.md."
echo "  Settings > Import from Markdown with ../SampleData2.md wipes the Swift app's data. Export a backup first."
if ! curl -sf "${BASE}/api/accounts/balances?format=text" | grep -q "Main Checking" \
    || ! curl -sf "${BASE}/api/accounts/balances?format=text" | grep -q "Rewards Card"; then
    echo "error: the balances do not mention Main Checking and Rewards Card. Load SampleData2.md first." >&2
    exit 1
fi
read -r -p "Loaded SampleData2.md into the Swift app? [y/N] " answer
if [[ ! "${answer}" =~ ^[Yy]$ ]]; then
    echo "Stopped: nothing captured."
    exit 1
fi

mkdir -p "${OUT}"
rm -f "${OUT}"/*.body

MANIFEST=""

# capture NAME PATH: GET the path, save the body, and add a manifest line.
capture() {
    local name="$1" path="$2" status_ct code ct
    status_ct="$(curl -s -o "${OUT}/${name}.body" -w '%{http_code}\t%{content_type}' "${BASE}${path}")"
    code="${status_ct%%$'\t'*}"
    ct="${status_ct#*$'\t'}"
    MANIFEST+="${name}"$'\t'"GET"$'\t'"${path}"$'\t'"${code}"$'\t'"${ct}"$'\n'
}

for fmt in json text markdown; do
    capture "balances_${fmt}" "/api/accounts/balances?format=${fmt}"
    capture "balances_${fmt}_scn" "/api/accounts/balances?format=${fmt}&scenario=${SCN}"
done
capture "scenarios_json" "/api/scenarios?format=json"
capture "scenarios_text" "/api/scenarios?format=text"
capture "scenarios_md" "/api/scenarios?format=markdown"
capture "balances_reality_ci" "/api/accounts/balances?format=json&scenario=REALITY"
capture "balances_bad_scenario_json" "/api/accounts/balances?format=json&scenario=Nope"
capture "balances_bad_scenario_text" "/api/accounts/balances?format=text&scenario=Nope"

for acct_pair in "main_checking:${MAIN}" "rewards_card:${REWARDS}"; do
    slug="${acct_pair%%:*}"
    enc="${acct_pair#*:}"
    for fmt in json text markdown; do
        capture "balance_${slug}_${fmt}" "/api/accounts/${enc}/balance?format=${fmt}"
        capture "tx_${slug}_${fmt}" "/api/accounts/${enc}/transactions?n=10&format=${fmt}"
        capture "tx_${slug}_${fmt}_scn" "/api/accounts/${enc}/transactions?n=10&format=${fmt}&scenario=${SCN}"
    done
    capture "tx_${slug}_all_json" "/api/accounts/${enc}/transactions?n=100000&format=json"
done

capture "balance_case_insensitive" "/api/accounts/main%20checking/balance"
capture "balance_plus_in_query" "/api/accounts/${MAIN}/balance?scenario=New+Car+Savings"
capture "balance_not_found" "/api/accounts/Nope/balance"
capture "tx_default_n" "/api/accounts/${MAIN}/transactions"
capture "unknown_route" "/api/nope"
capture "export_markdown" "/api/export/markdown"

printf 'name\tmethod\tpath\tstatus\tcontent-type\n%s' "${MANIFEST}" > "${OUT}/manifest.tsv"

TZ_NAME="$(readlink /etc/localtime | sed 's#.*/zoneinfo/##')"
SWIFT_COMMIT="$(git -C "${SWIFT_REPO}" rev-parse --short HEAD 2>/dev/null || echo unknown)"
{
    echo "captured=$(date +%Y-%m-%d)"
    echo "tz=${TZ_NAME}"
    echo "swift_commit=${SWIFT_COMMIT}"
    echo "fixture=SampleData2"
} > "${OUT}/meta.txt"

echo "Captured $(($(wc -l < "${OUT}/manifest.tsv") - 1)) replies into ${OUT} (see manifest.tsv and meta.txt)."
