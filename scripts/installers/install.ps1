# Installs the com CLI on Windows without Node, npm or Bun:
#   powershell -c "irm com.ai.vn/install.ps1 | iex"
# $env:COM_VERSION="1.0.2" for a given release. Runs the per-user setup silently (no admin rights):
# com goes into %LOCALAPPDATA%\Programs\com and onto the user's PATH.
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$base = if ($env:COM_DOWNLOAD_URL) { $env:COM_DOWNLOAD_URL } else { "https://hcm04.vstorage.vngcloud.vn/files/com-cli" }
$release = if ($env:COM_VERSION) { "v" + $env:COM_VERSION.TrimStart("v") } else { "latest" }
$file = "com-windows-setup.exe"

if (-not [Environment]::Is64BitOperatingSystem) { throw "com needs 64-bit Windows" }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("com-install-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $setup = Join-Path $tmp $file
  Write-Host "Downloading $base/$release/$file"
  Invoke-WebRequest -UseBasicParsing "$base/$release/$file" -OutFile $setup
  $sums = (Invoke-WebRequest -UseBasicParsing "$base/$release/SHA256SUMS").Content
  if ($sums -is [byte[]]) { $sums = [Text.Encoding]::UTF8.GetString($sums) }
  $expected = ($sums -split "`n" | Where-Object { $_ -match "\s$([regex]::Escape($file))\s*$" } | Select-Object -First 1) -replace "\s.*$", ""
  $actual = (Get-FileHash -Algorithm SHA256 $setup).Hash.ToLower()
  if (-not $expected -or $expected.ToLower() -ne $actual) { throw "Checksum mismatch for $file" }

  # A running com locks its files: the setup could not replace them
  Get-Process com-compiler -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

  $proc = Start-Process -FilePath $setup -ArgumentList "/S" -Wait -PassThru
  if ($proc.ExitCode -ne 0) { throw "$file exited with code $($proc.ExitCode)" }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

$dest = Join-Path $env:LOCALAPPDATA "Programs\com"
if (-not (Test-Path (Join-Path $dest "com.cmd"))) { throw "The setup did not install com into $dest" }
# The setup adds $dest to the user's PATH; this session sees it without reopening the terminal
if (($env:Path -split ";") -notcontains $dest) { $env:Path = "$dest;$env:Path" }
Write-Host "Installed com into $dest"
Write-Host "Run: com --version (open a new terminal if com is not found)"
