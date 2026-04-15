param(
    [Parameter(Mandatory = $false)]
    [string]$ArtifactPath = "",

    [Parameter(Mandatory = $false)]
    [string]$ArtifactUrl = "",

    [Parameter(Mandatory = $false)]
    [string]$GitHubRepo = "srv1n/pysandbox-rs",

    [Parameter(Mandatory = $false)]
    [string]$Version = "",

    [Parameter(Mandatory = $false)]
    [ValidateSet("system")]
    [string]$Variant = "system",

    [Parameter(Mandatory = $false)]
    [string]$InstallRoot = "$env:LOCALAPPDATA\RZN Python Tools",

    [Parameter(Mandatory = $false)]
    [string]$BinDir = "$env:LOCALAPPDATA\RZN Python Tools\bin",

    [Parameter(Mandatory = $false)]
    [string]$WorkflowsDir = "$env:USERPROFILE\.rzn\python-tools\workflows",

    [switch]$SkipWorkflowsSync
)

$ErrorActionPreference = "Stop"

function Get-PlatformKey {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()
    switch ($arch) {
        "x64"   { return "windows_x86_64" }
        "amd64" { return "windows_x86_64" }
        default { throw "Unsupported Windows architecture: $arch" }
    }
}

function Add-ToUserPath([string]$PathToAdd) {
    $current = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @()
    if ($current) {
        $parts = $current.Split(";") | Where-Object { $_ -ne "" }
    }
    if ($parts -contains $PathToAdd) {
        return
    }
    $newPath = ($parts + $PathToAdd) -join ";"
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
}

if (-not $ArtifactPath -and -not $ArtifactUrl) {
    if (-not $Version) {
        throw "Provide -ArtifactPath, -ArtifactUrl, or -Version."
    }
    $platform = Get-PlatformKey
    $ArtifactUrl = "https://github.com/$GitHubRepo/releases/download/v$Version/rzn-python-tools-$Version-$platform-$Variant.zip"
}

$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("rzn-python-tools-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot | Out-Null

try {
    $zipPath = Join-Path $tempRoot "artifact.zip"
    $artifactName = ""
    if ($ArtifactPath) {
        $artifactName = Split-Path -Leaf $ArtifactPath
        Copy-Item $ArtifactPath $zipPath
    } else {
        $artifactName = [System.IO.Path]::GetFileName(([Uri]$ArtifactUrl).AbsolutePath)
        Invoke-WebRequest -Uri $ArtifactUrl -OutFile $zipPath
    }

    $extractRoot = Join-Path $tempRoot "extract"
    Expand-Archive -Path $zipPath -DestinationPath $extractRoot -Force

    $packageDir = Join-Path $extractRoot "rzn-python-tools"
    if (-not (Test-Path $packageDir)) {
        throw "Invalid archive: missing rzn-python-tools directory"
    }

    $releaseName = [IO.Path]::GetFileNameWithoutExtension($artifactName)
    $releaseDir = Join-Path $InstallRoot "releases\$releaseName"
    $currentDir = Join-Path $InstallRoot "current"

    New-Item -ItemType Directory -Force -Path (Join-Path $InstallRoot "releases") | Out-Null
    if (Test-Path $releaseDir) {
        Remove-Item $releaseDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
    Copy-Item "$packageDir\*" $releaseDir -Recurse -Force

    if (Test-Path $currentDir) {
        Remove-Item $currentDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $currentDir | Out-Null
    Copy-Item "$releaseDir\*" $currentDir -Recurse -Force

    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    Copy-Item (Join-Path $currentDir "bin\rzn-python-tools.exe") (Join-Path $BinDir "rzn-python-tools.exe") -Force

    $workerWrapper = @"
@echo off
set "RZN_PYTHON_TOOLS_HOME=$currentDir"
set "RZN_PLUGIN_DIR=%RZN_PYTHON_TOOLS_HOME%"
"%RZN_PYTHON_TOOLS_HOME%\libexec\rzn-python-worker.exe" %*
"@
    Set-Content -Path (Join-Path $BinDir "rzn-python-worker.cmd") -Value $workerWrapper -Encoding ASCII

    if (-not $SkipWorkflowsSync) {
        New-Item -ItemType Directory -Force -Path $WorkflowsDir | Out-Null
        & (Join-Path $BinDir "rzn-python-tools.exe") workflows sync --dest $WorkflowsDir --force | Out-Null
    }

    Add-ToUserPath $BinDir

    Write-Host "installed rzn-python-tools into $currentDir"
    Write-Host "cli: $(Join-Path $BinDir 'rzn-python-tools.exe')"
    Write-Host "worker: $(Join-Path $BinDir 'rzn-python-worker.cmd')"
    Write-Host "workflows: $WorkflowsDir"
} finally {
    if (Test-Path $tempRoot) {
        Remove-Item $tempRoot -Recurse -Force
    }
}
