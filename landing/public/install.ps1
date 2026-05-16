$ErrorActionPreference = "Stop"

function Fail($Message) {
  Write-Error "xero-tui install: $Message"
  exit 1
}

function Get-TargetTriple {
  $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture

  switch ($arch) {
    "X64" { return "x86_64-pc-windows-msvc" }
    default { Fail "unsupported Windows architecture: $arch" }
  }
}

$baseUrl = if ($env:XERO_INSTALL_BASE_URL) { $env:XERO_INSTALL_BASE_URL.TrimEnd("/") } else { "https://xeroshell.com" }
$installDir = if ($env:XERO_INSTALL_DIR) { $env:XERO_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\Xero\bin" }
$target = Get-TargetTriple
$archive = "xero-tui-$target.zip"
$archiveUrl = "$baseUrl/downloads/tui/latest/$archive"
$checksumUrl = "$archiveUrl.sha256"
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) "xero-tui-install-$([System.Guid]::NewGuid().ToString('n'))"

New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

try {
  $archivePath = Join-Path $tempDir $archive
  $checksumPath = "$archivePath.sha256"
  $extractDir = Join-Path $tempDir "extract"

  Write-Host "Downloading $archiveUrl"
  Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath
  Invoke-WebRequest -Uri $checksumUrl -OutFile $checksumPath

  $expected = (Get-Content $checksumPath -Raw).Trim().Split(" ", [System.StringSplitOptions]::RemoveEmptyEntries)[0].ToLowerInvariant()
  $actual = (Get-FileHash -Algorithm SHA256 -Path $archivePath).Hash.ToLowerInvariant()
  if ($expected -ne $actual) {
    Fail "checksum mismatch for $archive"
  }

  Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force
  $binary = Join-Path $extractDir "xero-tui.exe"
  if (!(Test-Path $binary)) {
    Fail "archive did not contain xero-tui.exe"
  }

  New-Item -ItemType Directory -Force -Path $installDir | Out-Null
  Copy-Item -Path $binary -Destination (Join-Path $installDir "xero-tui.exe") -Force

  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  $pathEntries = @()
  if ($userPath) {
    $pathEntries = $userPath -split ";"
  }

  if (!($pathEntries | Where-Object { $_ -ieq $installDir })) {
    $nextPath = if ($userPath) { "$userPath;$installDir" } else { $installDir }
    [Environment]::SetEnvironmentVariable("Path", $nextPath, "User")
    $env:Path = "$env:Path;$installDir"
    Write-Host "Added $installDir to your user PATH. Open a new terminal to use it everywhere."
  }

  Write-Host "Installed xero-tui to $(Join-Path $installDir 'xero-tui.exe')"
  Write-Host "Run it with: xero-tui"
} finally {
  Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
}
