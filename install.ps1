# Install sshtui for the current user. Works with Windows PowerShell 5.1 and PowerShell 7.
& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    if ([Net.ServicePointManager]::SecurityProtocol -ne [Net.SecurityProtocolType]::SystemDefault) {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    }

    function Get-ReleaseFile([string]$Url, [string]$Destination) {
        $uri = [Uri]$Url
        if ($uri.IsFile) {
            Copy-Item -LiteralPath $uri.LocalPath -Destination $Destination
        } else {
            Invoke-WebRequest -Uri $uri -UseBasicParsing -OutFile $Destination
        }
    }

    function Add-InstallPath([string]$Directory) {
        $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
        $changed = $false
        try {
            $userPath = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $expanded = [Environment]::ExpandEnvironmentVariables($userPath)
            $directoryPath = $Directory.TrimEnd('\')
            if ($directoryPath -notin @($expanded -split ';' | ForEach-Object { $_.Trim().TrimEnd('\') })) {
                $key.SetValue('Path', (($userPath.TrimEnd(';'), $Directory | Where-Object { $_ }) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
                $changed = $true
            }
        } finally {
            $key.Dispose()
        }
        $sessionPath = [string]$env:Path
        if ($directoryPath -notin @($sessionPath -split ';' | ForEach-Object { $_.Trim().TrimEnd('\') })) {
            $env:Path = (($sessionPath.TrimEnd(';'), $Directory | Where-Object { $_ }) -join ';')
        }
        if ($changed) {
            if (-not ('Sshtui.NativeMethods' -as [type])) {
                Add-Type -Namespace Sshtui -Name NativeMethods -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr SendMessageTimeout(System.IntPtr window, uint message,
    System.UIntPtr wParam, string lParam, uint flags, uint timeout, out System.UIntPtr result);
'@
            }
            $result = [UIntPtr]::Zero
            # Broadcast WM_SETTINGCHANGE to notify other applications of the PATH update.
            $null = [Sshtui.NativeMethods]::SendMessageTimeout([IntPtr]0xffff, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
        }
    }

    function Confirm-Version([string]$Executable, [string]$Version) {
        $reported = & $Executable --version
        if ($LASTEXITCODE -ne 0 -or ($reported -join [Environment]::NewLine) -cne "sshtui $Version") {
            throw "The executable at $Executable did not report sshtui $Version."
        }
    }

    $repo = 'hungryZoo/sshtui'
    $releaseRoot = "https://github.com/$repo/releases"
    $version = $env:SSHTUI_VERSION
    $downloadBase = $env:SSHTUI_DOWNLOAD_BASE
    $versionPattern = '[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?'

    if ($downloadBase -and -not $version) {
        throw 'SSHTUI_DOWNLOAD_BASE requires SSHTUI_VERSION.'
    }
    if ($version) {
        $version = $version -creplace '^v', ''
        if ($version -cnotmatch "\A$versionPattern\z") {
            throw "Invalid release version: $version"
        }
    }

    $architecture = $env:SSHTUI_ARCH
    if (-not $architecture) {
        $machineEnvironment = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
        $architecture = (Get-ItemProperty -LiteralPath $machineEnvironment -ErrorAction SilentlyContinue).PROCESSOR_ARCHITECTURE
        if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITEW6432 }
        if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
    }
    $arch = switch ($architecture) {
        { $_ -in 'AMD64', 'x86_64', 'x64' } { 'x86_64' }
        { $_ -in 'ARM64', 'aarch64' } { 'aarch64' }
        default { throw "Unsupported Windows architecture: $architecture. Use Windows x64 or ARM64." }
    }
    $suffix = "$arch-pc-windows-msvc.zip"
    $escapedSuffix = [regex]::Escape($suffix)
    $installDir = $env:SSHTUI_INSTALL_DIR
    if (-not $installDir) {
        $installDir = Join-Path $env:LOCALAPPDATA 'Programs\sshtui\bin'
    }
    $installDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($installDir)

    $work = Join-Path ([IO.Path]::GetTempPath()) ('sshtui-' + [Guid]::NewGuid())
    $null = New-Item -ItemType Directory -Path $work
    $staged = $null
    try {
        $sums = Join-Path $work 'SHA256SUMS'
        if (-not $version) {
            Get-ReleaseFile "$releaseRoot/latest/download/SHA256SUMS" $sums
            $versions = @(foreach ($line in Get-Content -LiteralPath $sums) {
                if ($line -cmatch "^[a-fA-F0-9]{64}\s+\*?sshtui-($versionPattern)-$escapedSuffix\s*$") {
                    $Matches[1]
                }
            })
            if ($versions.Count -ne 1) {
                throw "Could not find the latest sshtui version for $arch in SHA256SUMS."
            }
            $version = $versions[0]
        }
        if (-not $downloadBase) {
            $downloadBase = "$releaseRoot/download/v$version"
        }
        $downloadBase = $downloadBase.TrimEnd('/')
        if (-not (Test-Path -LiteralPath $sums)) {
            Get-ReleaseFile "$downloadBase/SHA256SUMS" $sums
        }

        $asset = "sshtui-$version-$suffix"
        $archive = Join-Path $work $asset
        $escapedAsset = [regex]::Escape($asset)
        $expected = @(foreach ($line in Get-Content -LiteralPath $sums) {
            if ($line -cmatch "^([a-fA-F0-9]{64})\s+\*?$escapedAsset\s*$") {
                $Matches[1]
            }
        })
        if ($expected.Count -ne 1) {
            throw "Expected one SHA256 checksum for $asset."
        }

        Write-Host "Downloading $asset"
        Get-ReleaseFile "$downloadBase/$asset" $archive
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash -ine $expected[0]) {
            throw "SHA256 checksum mismatch for $asset."
        }
        $extracted = Join-Path $work 'extracted'
        Expand-Archive -LiteralPath $archive -DestinationPath $extracted -Force

        $null = [IO.Directory]::CreateDirectory($installDir)
        $destination = Join-Path $installDir 'sshtui.exe'
        $old = "$destination.old"
        $staged = Join-Path $installDir ("sshtui-" + [Guid]::NewGuid() + '.exe')
        [IO.File]::Copy((Join-Path $extracted 'sshtui.exe'), $staged)
        Confirm-Version $staged $version

        foreach ($backup in Get-ChildItem -LiteralPath $installDir -Filter 'sshtui.exe*.old' -File -Force) {
            try {
                [IO.File]::Delete($backup.FullName)
            } catch [IO.IOException], [UnauthorizedAccessException] {
                # A previous version may still be running.
            }
        }
        if ([IO.File]::Exists($destination)) {
            if (Test-Path -LiteralPath $old) {
                $old = "$destination.$([Guid]::NewGuid()).old"
            }
            try {
                [IO.File]::Replace($staged, $destination, $old)
            } catch [IO.IOException], [UnauthorizedAccessException] {
                # A running executable can prevent replacement but still allow a rename.
                [IO.File]::Move($destination, $old)
                try {
                    [IO.File]::Move($staged, $destination)
                } catch {
                    [IO.File]::Move($old, $destination)
                    throw
                }
            }
        } else {
            [IO.File]::Move($staged, $destination)
        }
        Confirm-Version $destination $version
        Write-Host "Installed sshtui $version to $destination"

        if ($env:SSHTUI_NO_MODIFY_PATH -in '', $null, '0') {
            Add-InstallPath $installDir
        }
        if (-not (Get-Command ssh.exe -CommandType Application -ErrorAction SilentlyContinue)) {
            Write-Warning 'ssh.exe was not found on PATH. Install the OpenSSH Client from Windows Optional Features.'
        }
    } finally {
        if ($staged) {
            Remove-Item -LiteralPath $staged -Force -ErrorAction SilentlyContinue
        }
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}
