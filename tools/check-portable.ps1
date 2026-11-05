# PowerShell twin of check-portable.sh: wasm32 check plus a scan for forbidden APIs in parsing-crate source.
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$crates = 'model','zip','seg','text','json','markdown','html','po','subtitle','docx','xlsx','pptx','xliff','sdl'
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
        $n = 0
        foreach ($line in Get-Content $_.FullName) {
            $n++
            if ($line -match '#\[cfg\(test\)\]') { break }
            if ($line -match '^\s*//') { continue }
            if ($line -match $pattern) { Write-Host "$($_.FullName):${n}: $line"; $fail = $true }
        }
    }
}
if ($fail) { Write-Host 'portable: FAILED'; exit 1 } else { Write-Host 'portable: OK' }
