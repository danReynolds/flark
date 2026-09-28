#Requires -Version 5.1
<#
.SYNOPSIS
    Collects log files into an archive, written to exercise the highlighter.
.PARAMETER Path
    Where the logs live. # a hash inside the block comment
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateNotNullOrEmpty()]
    [string] $Path,

    [int] $MaxAgeDays = 30,
    [switch] $WhatIfOnly,
    [string[]] $Include = @('*.log', '*.txt')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-OldLog {
    param([string] $Root, [datetime] $Before)
    Get-ChildItem -Path $Root -Recurse -File |
        Where-Object { $_.LastWriteTime -lt $Before -and $_.Length -gt 0 } |
        Sort-Object -Property Length -Descending
}

$cutoff = (Get-Date).AddDays(-$MaxAgeDays)
$logs = @(Get-OldLog -Root $Path -Before $cutoff)
$total = ($logs | Measure-Object -Property Length -Sum).Sum
Write-Host "Found $($logs.Count) files, $([math]::Round($total / 1MB, 2)) MB, older than $cutoff"
Write-Verbose "Root is ${Path} and user is $env:USERNAME"

$sizes = @{
    Small  = 512KB
    Medium = 0x10 * 1mb
    Large  = 1.5e3
}
$sizes['Huge'] = 2GB

$splat = @{
    Path        = Join-Path $Path 'archive.zip'
    Force       = $true
    ErrorAction = 'SilentlyContinue'
}
Remove-Item @splat
Compress-Archive -Path $logs.FullName -DestinationPath $splat.Path -WhatIf:$WhatIfOnly

$message = @"
Archive report for $Path
  files: $($logs.Count)
  owner: $env:USERNAME, escaped `$notAVariable
"@
$literal = @'
Nothing $expands here, not even $(this).
'@

switch -Regex ($message) {
    '^Archive' { 'starts well'; break }
    'files: \d+' { "counted" }
    default { Write-Warning 'unexpected' }
}

foreach ($log in $logs) {
    if ($log.Name -like '*.tmp' -or $log.Extension -eq '.bak') {
        continue
    } elseif ($log.Name -match '^(\w+)-(\d{4})' -and -not $log.PSIsContainer) {
        $name = $Matches[1] -replace '_', '-'
        '{0,-20} {1,10:N0}' -f $name, $log.Length
    } else {
        $parts = $log.BaseName -split '\.'
        $joined = $parts -join '/'
    }
}

$numbers = 1..10 | ForEach-Object { $_ * 2 } | Where-Object { $_ % 3 -ne 0 }
$ok = $numbers -contains 4 -and $numbers.Count -ge 3
$type = 42 -is [int]
$text = '5' -as [int]
$quoted = 'It''s a single-quoted string'
$doubled = "She said ""hello"" and left"
$pi = [Math]::PI
$scriptBlock = { param($x) return $x * $x }
& $scriptBlock 3
$null = New-Item -ItemType Directory -Path "$env:TEMP\logs" -Force

try {
    $response = Invoke-RestMethod -Uri 'https://example.com/api' -Method Get
    $response.items | Select-Object -First 5 | Format-Table -AutoSize
}
catch [System.Net.WebException] {
    Write-Error "Request failed: $($_.Exception.Message)"
}
finally {
    Pop-Location
}

$counter = 0
do {
    $counter++
    Start-Sleep -Milliseconds 100
} until ($counter -ge 3)

while ($true) { break }
$hash = @{ a = 1; b = $false; c = $null }
$items = @(1, 2, 3)
$last = $items[-1]
Get-Process | ? { $_.CPU -gt 100 } | % { $_.Name }
cd C:\Windows; ls; cat .\notes.txt
exit $LASTEXITCODE
