# FluxGuard installer for Windows.
#
#   irm https://raw.githubusercontent.com/suiflex/FluxGuard/develop/scripts/install.ps1 | iex
#
# Environment overrides:
#   FLUXGUARD_VERSION      release tag to install (default: latest)
#   FLUXGUARD_INSTALL_DIR  destination directory (default: %LOCALAPPDATA%\Programs\FluxGuard\bin)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = "suiflex/FluxGuard"

function Die($message) {
    Write-Error "fluxguard: $message"
    exit 1
}

function Get-Asset {
    $machine = $env:PROCESSOR_ARCHITEW6432
    if (-not $machine) { $machine = $env:PROCESSOR_ARCHITECTURE }
    switch ($machine) {
        "AMD64" { return "fluxguard-windows-x86_64" }
        "ARM64" { return "fluxguard-windows-aarch64" }
        default { Die "unsupported architecture: $machine" }
    }
}

function Get-LatestVersion {
    try {
        $response = Invoke-WebRequest -Uri "https://github.com/$Repo/releases/latest" `
            -MaximumRedirection 0 -ErrorAction SilentlyContinue -UseBasicParsing
        $location = $response.Headers.Location
    } catch {
        $location = $_.Exception.Response.Headers.Location
    }
    if (-not $location) { Die "could not reach GitHub to resolve the latest release" }
    $tag = ([string]$location).Split("/")[-1]
    if (-not $tag -or $tag -eq "releases") { Die "no published release found for $Repo" }
    return $tag
}

$asset = Get-Asset
$version = if ($env:FLUXGUARD_VERSION) { $env:FLUXGUARD_VERSION } else { Get-LatestVersion }
$installDir = if ($env:FLUXGUARD_INSTALL_DIR) {
    $env:FLUXGUARD_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "Programs\FluxGuard\bin"
}
$base = "https://github.com/$Repo/releases/download/$version"

$scratch = Join-Path ([IO.Path]::GetTempPath()) ("fluxguard-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $scratch -Force | Out-Null
try {
    $archive = "$asset.zip"
    $archivePath = Join-Path $scratch $archive
    $sumsPath = Join-Path $scratch "SHA256SUMS"

    Write-Host "Downloading $archive $version"
    try {
        Invoke-WebRequest -Uri "$base/$archive" -OutFile $archivePath -UseBasicParsing
    } catch {
        Die "no asset '$archive' in release $version; see https://github.com/$Repo/releases"
    }
    try {
        Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile $sumsPath -UseBasicParsing
    } catch {
        Die "release $version has no SHA256SUMS; refusing to install an unverified binary"
    }

    $entry = Get-Content $sumsPath | Where-Object { $_ -match "[ *]$([regex]::Escape($archive))$" }
    if (-not $entry) { Die "SHA256SUMS has no entry for $archive" }
    $expected = ($entry -split '\s+')[0]
    $actual = (Get-FileHash -Path $archivePath -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected.ToLower()) {
        Die "checksum mismatch for $archive (expected $expected, got $actual)"
    }

    Expand-Archive -Path $archivePath -DestinationPath $scratch -Force
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    $target = Join-Path $installDir "fluxguard.exe"
    Copy-Item -Path (Join-Path $scratch "fluxguard.exe") -Destination $target -Force
    try { Unblock-File -Path $target } catch {}

    Write-Host "Installed fluxguard $version to $target"

    $paths = $env:PATH -split ";" | ForEach-Object { $_.TrimEnd("\") }
    if ($paths -notcontains $installDir.TrimEnd("\")) {
        [Environment]::SetEnvironmentVariable(
            "PATH",
            "$([Environment]::GetEnvironmentVariable('PATH', 'User'));$installDir",
            "User"
        )
        Write-Warning "Added $installDir to User PATH. Restart your shell for changes to take effect."
    }
}
finally {
    Remove-Item -Path $scratch -Recurse -Force -ErrorAction SilentlyContinue
}
