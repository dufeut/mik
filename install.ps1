# mik installer for Windows
# Usage: powershell -ExecutionPolicy ByPass -c "irm https://raw.githubusercontent.com/dufeut/mik/main/install.ps1 | iex"

$ErrorActionPreference = "Stop"

$Repo = "dufeut/mik"
$InstallDir = if ($env:MIK_INSTALL_DIR) { $env:MIK_INSTALL_DIR } else { "$env:USERPROFILE\.mik\bin" }

function Get-LatestVersion {
    $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    return $response.tag_name -replace '^v', ''
}

function Install-Mik {
    $Version = Get-LatestVersion
    $Platform = "x86_64-pc-windows-msvc"

    Write-Host "Installing mik v$Version for $Platform..."

    $DownloadUrl = "https://github.com/$Repo/releases/download/v$Version/mik-$Platform.zip"
    $TempDir = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "mik-install-$(Get-Random)")
    $TempFile = Join-Path $TempDir "mik.zip"

    Write-Host "Downloading from $DownloadUrl..."
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $TempFile

    Write-Host "Extracting..."
    Expand-Archive -Path $TempFile -DestinationPath $TempDir -Force

    Write-Host "Installing to $InstallDir..."
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Move-Item -Path (Join-Path $TempDir "mik.exe") -Destination (Join-Path $InstallDir "mik.exe") -Force

    Remove-Item -Path $TempDir -Recurse -Force

    # Add to PATH if not already there
    $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($CurrentPath -notlike "*$InstallDir*") {
        Write-Host ""
        Write-Host "Adding $InstallDir to your PATH..."
        [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$CurrentPath", "User")
        $env:Path = "$InstallDir;$env:Path"
    }

    Write-Host ""
    Write-Host "mik v$Version installed successfully!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Restart your terminal to use mik, or run:"
    Write-Host ""
    Write-Host "  `$env:Path = `"$InstallDir;`$env:Path`""
    Write-Host ""
}

Install-Mik
