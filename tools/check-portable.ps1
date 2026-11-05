# PowerShell twin of check-portable.sh: wasm32 check plus a scan for forbidden APIs in parsing-crate source.
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$crates = 'model','zip','seg','text','json','markdown','html','po','subtitle','docx','xlsx','pptx','xliff','sdl','ini','strings','toml','properties','resjson','json5','jsmodule','yaml','arb','resx','xaml','dtd','wxl','ftl','xcstrings','xmb','qtts','php','plist','xml','rc','mo','csv'
$pkgs = @(); foreach ($c in $crates) { $pkgs += '-p'; $pkgs += "bilingua-$c" }
$fail = $false

if (-not ((rustup target list --installed) -contains 'wasm32-unknown-unknown')) {
    Write-Host 'missing target: run  rustup target add wasm32-unknown-unknown'; exit 2
}
Write-Host '== cargo check --target wasm32-unknown-unknown =='
cargo check --target wasm32-unknown-unknown @pkgs -p bilingua
if ($LASTEXITCODE -ne 0) { $fail = $true }

Write-Host '== forbidden APIs in parsing-crate source (tests excluded) =='
$pattern = 'std::fs|std::thread|std::net|std::process|std::env|SystemTime|Instant::now|std::time::Instant|tokio|reqwest|rayon'
$dirs = ($crates | ForEach-Object { "bilingua-$_" }) + 'bilingua'
foreach ($d in $dirs) {
    Get-ChildItem -Path (Join-Path $d 'src') -Filter *.rs -Recurse | Where-Object {
        $_.Name -notmatch '^tests(_.*)?\.rs$'
    } | ForEach-Object {
        # Same rule as tools/non-test-lines.awk: a column-0 #[cfg(test)] before an inline `mod name {` skips that
        # module up to its column-0 `}`; any other #[cfg(test)] skips only the next line.
        $n = 0; $skip = $false; $pend = 0
        foreach ($line in Get-Content $_.FullName) {
            $n++
            if ($skip) { if ($line -match '^\}') { $skip = $false }; continue }
            if ($pend -gt 0) {
                if ($pend -eq 2 -and $line -match '^(pub(\([a-z]+\))? )?mod [A-Za-z_0-9]+ *\{ *$') { $skip = $true }
                $pend = 0; continue
            }
            if ($line -match '^#\[cfg\(test\)\]') { $pend = 2; continue }
            if ($line -match '^\s+#\[cfg\(test\)\]') { $pend = 1; continue }
            if ($line -match '^\s*//') { continue }
            if ($line -match $pattern) { Write-Host "$($_.FullName):${n}: $line"; $fail = $true }
        }
    }
}
if ($fail) { Write-Host 'portable: FAILED'; exit 1 } else { Write-Host 'portable: OK' }
