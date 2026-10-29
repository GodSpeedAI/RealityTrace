# Functional smoke of an INSTALLED (packaged) realitytrace on Windows.
#
# Usage: packaging/windows/smoke.ps1 <realitytrace.exe> <expected-version>
#
# Uses a disposable HOME / XDG roots so it never touches real user data. It
# exercises the evidence and ledger semantics, not just startup:
#   version, doctor JSON, explicit daemon refusal (Unix-socket transport is
#   unavailable on Windows), enrollment, expectation-before-observation,
#   supporting run settles, contradicting run does NOT, ledger verification,
#   tamper detection, immutable repository, and fail-closed on a lost ledger.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Binary,
    [Parameter(Mandatory = $true, Position = 1)][string]$Version
)

$ErrorActionPreference = 'Stop'
$RT = (Resolve-Path $Binary).Path

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("rt-smoke-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $work | Out-Null

function Fail([string]$Message) {
    Write-Host "smoke: FAIL $Message" -ForegroundColor Red
    exit 1
}
function Pass([string]$Message) {
    Write-Host "smoke: PASS $Message" -ForegroundColor Green
}
function Invoke-RT {
    # Runs realitytrace and returns parsed JSON; throws on a non-zero exit.
    param([string[]]$Arguments)
    $out = & $RT @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "realitytrace $($Arguments -join ' ') exited $LASTEXITCODE`n$out"
    }
    return ($out | Out-String | ConvertFrom-Json)
}

try {
    $env:HOME = Join-Path $work 'home'
    $env:XDG_DATA_HOME = Join-Path $work 'xdg-data'
    $env:XDG_STATE_HOME = Join-Path $work 'xdg-state'
    Remove-Item Env:XDG_RUNTIME_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:SXR_DAEMON_SOCKET -ErrorAction SilentlyContinue
    Remove-Item Env:REALITYTRACE_DOMAINFORGE_BIN -ErrorAction SilentlyContinue
    Remove-Item Env:SXR_DOMAINFORGE_BIN -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $env:HOME, $env:XDG_DATA_HOME, $env:XDG_STATE_HOME | Out-Null

    Write-Host "smoke: binary $RT"
    Write-Host "smoke: fresh HOME $env:HOME"

    # 1. version
    $got = (& $RT --version | Out-String).Trim()
    if ($got -ne "realitytrace $Version") { Fail "version: got '$got', want 'realitytrace $Version'" }
    Pass "version -> $got"

    # 2. doctor --json
    $doctor = Invoke-RT @('doctor', '--json')
    if (-not $doctor.data_root.StartsWith($env:XDG_DATA_HOME)) { Fail "data_root '$($doctor.data_root)' is outside the disposable root" }
    if (-not $doctor.state_root.StartsWith($env:XDG_STATE_HOME)) { Fail "state_root '$($doctor.state_root)' is outside the disposable root" }
    if ($null -eq $doctor.daemon_active) { Fail "doctor has no daemon_active field" }
    Pass "doctor --json valid, roots inside the disposable tree"

    # 3. the daemon is explicitly unsupported, never silently degraded
    $daemonOut = & $RT daemon 2>&1 | Out-String
    $daemonRc = $LASTEXITCODE
    if ($daemonRc -eq 0) { Fail "daemon exited 0 on Windows; it must refuse explicitly" }
    if ($daemonOut -notmatch 'not supported') { Fail "daemon refusal did not explain the platform limitation: $daemonOut" }
    Pass "daemon explicitly refused on Windows (exit $daemonRc)"

    # 4. enroll a disposable git repository
    $repo = Join-Path $work 'repo'
    New-Item -ItemType Directory -Force -Path (Join-Path $repo 'src') | Out-Null
    Set-Content -Path (Join-Path $repo 'src\lib.rs') -Value 'pub fn answer() -> u32 { 42 }' -Encoding ASCII
    Set-Content -Path (Join-Path $repo 'README.md') -Value '# smoke repo' -Encoding ASCII
    git -C $repo init -q
    git -C $repo add -A
    git -C $repo -c user.email=ci@example.invalid -c user.name=ci commit -qm init
    if ($LASTEXITCODE -ne 0) { Fail "could not create the disposable git repository" }

    $enroll = Invoke-RT @('repo', 'enroll', $repo, '--json')
    $rid = $enroll.repo_id
    if (-not $rid) { Fail "repo enroll returned no repo_id" }
    Pass "enrolled $rid"

    $identity = (Invoke-RT @('repo', 'identity', $rid, '--json')).identity
    if ($identity.repo_id -ne $rid) { Fail "identity repo_id mismatch" }
    if (-not $identity.checkout_id) { Fail "identity has no checkout_id" }
    Pass "repository identity verified"

    $before = (git -C $repo status --porcelain --ignored | Out-String).Trim()
    if ($before -ne '') { Fail "repo already dirty before workflow: $before" }

    # 5. expectation before observation
    $q = Join-Path $work 'question.json'
    $t = Join-Path $work 'threshold.json'
    '{"question_id":"q-smoke","question_form":"is_true","question_type":"verification","target_entities":["concept:smoke"],"expected_answer_shape":"boolean"}' | Set-Content -Path $q -Encoding ASCII
    '{"threshold_id":"t-smoke","minimum_relevance":"medium","minimum_reliability":"medium"}' | Set-Content -Path $t -Encoding ASCII
    Invoke-RT @('question', 'declare', '--repo', $rid, '--file', $q, '--json') | Out-Null
    Invoke-RT @('threshold', 'declare', '--repo', $rid, '--question', 'q-smoke', '--file', $t, '--json') | Out-Null
    Pass "question and threshold declared before any observation"

    function Invoke-Run([string]$ArgvJson) {
        $run = Invoke-RT @('run', 'begin', '--repo', $rid, '--snapshot', 'sha256:smoke', '--question', 'q-smoke', '--threshold', 't-smoke', '--json')
        if (-not $run.run_id) { throw 'run begin returned no run_id' }
        Invoke-RT @('plan', 'create', '--run', $run.run_id, '--verifier', 'command', '--target', 'concept:smoke', '--argv-json', $ArgvJson, '--cwd', '.', '--timeout-s', '60', '--json') | Out-Null
        $verify = Invoke-RT @('verify', 'run', '--run', $run.run_id, '--json')
        return @{ run_id = $run.run_id; observation = $verify.observations[0] }
    }

    function Get-LedgerDirection([string]$EvidenceId) {
        $rows = Invoke-RT @('ledger', 'export', '--repo', $rid)
        foreach ($row in $rows) {
            if ($row.record_kind -ne 'evidence.created') { continue }
            foreach ($ev in $row.payload.evidence) {
                if ($ev.evidence_id -eq $EvidenceId) { return $ev.direction }
            }
        }
        throw "evidence $EvidenceId not found in the ledger"
    }

    # 6. supporting observation settles
    $ok = Invoke-Run '["cmd","/c","exit 0"]'
    if ($ok.observation.exit_code -ne 0) { Fail "supporting observation exit_code=$($ok.observation.exit_code)" }
    $evOk = (Invoke-RT @('evidence', 'create', '--run', $ok.run_id, '--observation', $ok.observation.observation_id, '--derive', '--json')).evidence_id
    $dirOk = Get-LedgerDirection $evOk
    if ($dirOk -ne 'supports') { Fail "supporting evidence direction is '$dirOk', expected supports" }
    $settleOk = Invoke-RT @('settlement', 'register', '--run', $ok.run_id, '--question', 'q-smoke', '--threshold', 't-smoke', '--transformation', 'transformation:smoke', '--json')
    if ($settleOk.settlement_status -ne 'settled') { Fail "supporting run settled as '$($settleOk.settlement_status)'" }
    Pass "supporting observation -> evidence supports -> settlement settled"

    # 7. contradicting observation must not settle
    $bad = Invoke-Run '["cmd","/c","exit 1"]'
    if ($bad.observation.exit_code -ne 1) { Fail "failing observation exit_code=$($bad.observation.exit_code), expected 1" }
    $evBad = (Invoke-RT @('evidence', 'create', '--run', $bad.run_id, '--observation', $bad.observation.observation_id, '--derive', '--json')).evidence_id
    $dirBad = Get-LedgerDirection $evBad
    if ($dirBad -ne 'contradicts') { Fail "failing evidence direction is '$dirBad', expected contradicts" }
    $settleBad = Invoke-RT @('settlement', 'register', '--run', $bad.run_id, '--question', 'q-smoke', '--threshold', 't-smoke', '--transformation', 'transformation:smoke', '--json')
    if ($settleBad.settlement_status -ne 'unsettled') { Fail "failing run settled as '$($settleBad.settlement_status)' — a failure must never be reported as success" }
    if ($settleBad.unresolved_contradictions.Count -lt 1) { Fail "failing run has no unresolved contradiction" }
    Pass "failing observation -> evidence contradicts -> settlement unsettled (not success)"

    # 8. ledger integrity
    $verifyLedger = Invoke-RT @('ledger', 'verify', '--repo', $rid, '--json')
    if ($verifyLedger.ok -ne $true) { Fail "ledger verify ok=$($verifyLedger.ok)" }
    if ($verifyLedger.verified_count -lt 8) { Fail "verified_count=$($verifyLedger.verified_count), expected >= 8" }
    $rows = Invoke-RT @('ledger', 'export', '--repo', $rid)
    $kinds = @($rows | ForEach-Object { $_.record_kind })
    foreach ($want in @('observation.recorded', 'evidence.created', 'settlement.registered')) {
        if ($kinds -notcontains $want) { Fail "ledger export lacks $want" }
    }
    Pass "ledger verified ($($verifyLedger.verified_count) entries) with the required records"

    # 9. tamper evidence (needs python's sqlite3; the runner provides it)
    $python = Get-Command python -ErrorAction SilentlyContinue
    if (-not $python) { Fail "python is required for the tamper-evidence check" }
    $db = Join-Path $env:XDG_DATA_HOME "sxr\repos\$rid\ledger.db"
    if (-not (Test-Path $db)) { Fail "no ledger.db under $db" }
    & python -c "import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute(\"UPDATE ledger SET payload='{\\\"tampered\\\":true}' WHERE append_ordinal=(SELECT MAX(append_ordinal) FROM ledger)\"); c.commit(); c.close()" $db
    if ($LASTEXITCODE -ne 0) { Fail "could not tamper with the ledger for the negative test" }
    & $RT ledger verify --repo $rid --json *> $null
    if ($LASTEXITCODE -eq 0) { Fail "a tampered committed payload verified as intact" }
    Pass "tampered payload rejected by ledger verify"

    # 10. enrolled repository untouched
    $after = (git -C $repo status --porcelain --ignored | Out-String).Trim()
    if ($after -ne $before) { Fail "workflow modified the enrolled repository: $after" }
    Pass "enrolled repository unmodified"

    # 11. a lost ledger must not verify as an empty, intact chain (last)
    Get-ChildItem -Path (Join-Path $env:XDG_DATA_HOME "sxr\repos\$rid") -Filter 'ledger.db*' | Remove-Item -Force
    & $RT ledger verify --repo $rid --json *> $null
    if ($LASTEXITCODE -eq 0) { Fail "a deleted ledger verified as an empty, intact chain" }
    Pass "missing ledger fails verification"

    Write-Host 'smoke: ALL CHECKS PASSED'
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
