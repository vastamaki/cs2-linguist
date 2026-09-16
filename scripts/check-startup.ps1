param([Parameter(Mandatory = $true)][string]$Executable)

$ErrorActionPreference = 'Stop'
$log = Join-Path $env:LOCALAPPDATA 'dev.linguist.cs2/startup.log'

# Exercise both first launch and reopening with the settings saved by setup.
foreach ($attempt in 1..2) {
    $process = Start-Process -FilePath $Executable -PassThru
    try {
        $deadline = (Get-Date).AddSeconds(30)
        $ready = $false
        while ((Get-Date) -lt $deadline) {
            $process.Refresh()
            if ($process.HasExited) {
                throw "Linguist exited during startup (code $($process.ExitCode))."
            }
            $text = if (Test-Path $log) { [string](Get-Content $log -Raw) } else { '' }
            if ($text.Contains("process $($process.Id)") -and
                $text.Contains('Startup complete') -and
                $process.MainWindowHandle -ne 0 -and
                $process.MainWindowTitle -like 'Linguist*CS2 captions') {
                $ready = $true
                break
            }
            Start-Sleep -Milliseconds 250
        }
        if (-not $ready) { throw 'Linguist did not finish startup and show its main window.' }
        # Allow the WebViews' initial snapshot requests to finish too.
        Start-Sleep -Seconds 3
        $process.Refresh()
        if ($process.HasExited -or (Get-Content $log -Raw).Contains('Fatal error:')) {
            throw 'Linguist failed after initializing its windows.'
        }
        Write-Output "Startup attempt $attempt passed."
    } finally {
        if (Test-Path $log) { Get-Content $log }
        if (-not $process.HasExited) {
            Stop-Process -Id $process.Id -Force
            $process.WaitForExit()
        }
    }
}
