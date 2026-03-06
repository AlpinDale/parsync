$ErrorActionPreference = "Stop"

$Repo = if ($env:PARSYNC_REPO) { $env:PARSYNC_REPO } else { "AlpinDale/parsync" }
$InstallDir = if ($env:PARSYNC_INSTALL_DIR) { $env:PARSYNC_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\parsync\bin" }
$BinName = "parsync.exe"

$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($arch -ne [System.Runtime.InteropServices.Architecture]::X64) {
    throw "Unsupported Windows architecture: $arch. Only x86_64 Windows binaries are published."
}

$apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
$headers = @{ "User-Agent" = "parsync-installer" }
$release = Invoke-RestMethod -Uri $apiUrl -Headers $headers
$tag = $release.tag_name
if ([string]::IsNullOrWhiteSpace($tag)) {
    throw "Failed to read latest release tag from $apiUrl"
}

$assetName = "parsync-$tag-x86_64-windows.zip"
$asset = $release.assets | Where-Object { $_.name -eq $assetName } | Select-Object -First 1
$downloadUrl = if ($asset) { $asset.browser_download_url } else { "https://github.com/$Repo/releases/download/$tag/$assetName" }

$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("parsync-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmpRoot | Out-Null
try {
    $zipPath = Join-Path $tmpRoot $assetName
    Write-Host "[parsync] downloading $downloadUrl"
    Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -Headers $headers

    Expand-Archive -Path $zipPath -DestinationPath $tmpRoot -Force
    $srcExe = Join-Path $tmpRoot $BinName
    if (-not (Test-Path $srcExe)) {
        throw "Archive did not contain $BinName"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item $srcExe (Join-Path $InstallDir $BinName) -Force

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $userPath) { $userPath = "" }
    $pathParts = $userPath -split ';' | Where-Object { $_ -ne "" }
    if ($pathParts -notcontains $InstallDir) {
        $newPath = if ($userPath.TrimEnd(';')) { "$($userPath.TrimEnd(';'));$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        if (-not ($env:Path -split ';' | Where-Object { $_ -eq $InstallDir })) {
            $env:Path = "$env:Path;$InstallDir"
        }
        Write-Host "[parsync] added $InstallDir to user PATH"
    }

    $installedExe = Join-Path $InstallDir $BinName
    Write-Host "[parsync] installed to $installedExe"
    & $installedExe --version
}
finally {
    Remove-Item -Path $tmpRoot -Recurse -Force -ErrorAction SilentlyContinue
}
