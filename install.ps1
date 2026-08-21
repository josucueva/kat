<#
.SYNOPSIS
    Installs or uninstalls KAT (Knowledge Abstraction Tracker) binary and shell completions on Windows.

.DESCRIPTION
    Downloads the prebuilt kat.exe release or builds from source, copies to the destination directory,
    and updates the user's PATH environment variable.

.EXAMPLE
    irm https://raw.githubusercontent.com/josucueva/kat/main/install.ps1 | iex
    .\install.ps1 -Prefix "$HOME\.local"
    .\install.ps1 -Uninstall
#>

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Prefix = "$HOME\.local",

    [string]$RepoUrl = "https://github.com/josucueva/kat",

    [switch]$Uninstall,

    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help) {
    Write-Host "Usage: .\install.ps1 [-Prefix PATH] [-Uninstall] [-Help]"
    Write-Host "Installs or uninstalls KAT (Knowledge Abstraction Tracker) on Windows."
    Write-Host "Default prefix: $HOME\.local"
    Write-Host ""
    Write-Host "One-command install:"
    Write-Host "  irm https://raw.githubusercontent.com/josucueva/kat/main/install.ps1 | iex"
    exit 0
}

$BinDir = Join-Path $Prefix "bin"
$ExePath = Join-Path $BinDir "kat.exe"

# -----------------------------------------------------------------------------
# Uninstallation
# -----------------------------------------------------------------------------
if ($Uninstall) {
    Write-Host "Uninstalling KAT from $Prefix..."

    if (Test-Path $ExePath) {
        Write-Host "==> Removing binary $ExePath..."
        Remove-Item -Force $ExePath
    }

    Write-Host ""
    Write-Host "Uninstallation complete!"
    exit 0
}

# -----------------------------------------------------------------------------
# Installation
# -----------------------------------------------------------------------------
Write-Host "Installing KAT to $Prefix..."

$ScriptDir = ""
if ($PSScriptRoot) {
    $ScriptDir = $PSScriptRoot
} elseif ($MyInvocation.MyCommand.Path) {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
}

$IsInRepo = $false
if ($ScriptDir -and (Test-Path (Join-Path $ScriptDir "Cargo.toml"))) {
    $cargoContent = Get-Content (Join-Path $ScriptDir "Cargo.toml") -Raw -ErrorAction SilentlyContinue
    if ($cargoContent -match 'name\s*=\s*"kat"') {
        $IsInRepo = $true
    }
} elseif (Test-Path "Cargo.toml") {
    $cargoContent = Get-Content "Cargo.toml" -Raw -ErrorAction SilentlyContinue
    if ($cargoContent -match 'name\s*=\s*"kat"') {
        $IsInRepo = $true
    }
}

if ($IsInRepo) {
    # 1. Build and install from local repository
    Write-Host "==> Building KAT release binary..."
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Error "'cargo' (Rust toolchain) is required to build KAT from source. Install Rust via https://rustup.rs"
        exit 1
    }

    cargo build --release
    cargo run --bin generate_assets

    if (-not (Test-Path $BinDir)) {
        New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    }

    Write-Host "==> Installing binary to $ExePath..."
    Copy-Item -Force "target\release\kat.exe" $ExePath
} else {
    # 2. Remote / Standalone installation: download prebuilt GitHub release
    $target = "x86_64-pc-windows-msvc"
    $archiveName = "kat-$target.zip"
    $releaseUrl = "$RepoUrl/releases/latest/download/$archiveName"

    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("kat-install-" + [System.Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

    try {
        $zipPath = Join-Path $tempDir $archiveName
        Write-Host "==> Downloading prebuilt KAT release ($target)..."

        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri $releaseUrl -OutFile $zipPath -UseBasicParsing

        Write-Host "==> Extracting release archive..."
        Expand-Archive -Path $zipPath -DestinationPath $tempDir -Force

        $extractedDir = Join-Path $tempDir "kat-$target"
        $extractedExe = Join-Path $extractedDir "kat.exe"
        if (-not (Test-Path $extractedExe)) {
            $extractedExe = Join-Path $tempDir "kat.exe"
        }

        if (-not (Test-Path $extractedExe)) {
            throw "Failed to locate kat.exe in extracted release archive."
        }

        if (-not (Test-Path $BinDir)) {
            New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
        }

        Write-Host "==> Installing binary to $ExePath..."
        Copy-Item -Force $extractedExe $ExePath
    } catch {
        Write-Warning "Could not download prebuilt release: $_"
        Write-Host "==> Attempting fallback: build from source with cargo..."

        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            Write-Error "Failed to install prebuilt binary and 'cargo' is not installed. Please install Rust from https://rustup.rs or download kat from $RepoUrl/releases"
            exit 1
        }

        cargo install --git "$RepoUrl.git"
        Write-Host ""
        Write-Host "Installed kat via cargo to $env:USERPROFILE\.cargo\bin\kat.exe"
        exit 0
    } finally {
        if (Test-Path $tempDir) {
            Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
        }
    }
}

# -----------------------------------------------------------------------------
# Ensure PATH contains BinDir
# -----------------------------------------------------------------------------
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$userPathParts = ($userPath -split ';') | Where-Object { $_ -ne "" }

if ($userPathParts -notcontains $BinDir) {
    Write-Host "==> Adding $BinDir to User PATH..."
    $newPath = ($userPathParts + $BinDir) -join ';'
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    $env:Path = "$env:Path;$BinDir"
    $PathUpdated = $true
} else {
    $PathUpdated = $false
}

Write-Host ""
Write-Host "Installation complete!"
Write-Host "Binary: $ExePath"
Write-Host ""

if ($PathUpdated) {
    Write-Host "NOTE: $BinDir was added to your PATH."
    Write-Host "Restart your PowerShell terminal to use 'kat' from anywhere."
} else {
    Write-Host "Ensure $BinDir is in your PATH."
}
