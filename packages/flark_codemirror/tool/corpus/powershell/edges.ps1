#region Edge cases for strings, variables and operators
<# A block comment that ends mid-line #> Write-Output 'after'
<#> not closed by the same hash
  still a comment #>
$unterminated = 'no closing quote
$double = "interpolation $name continues
  onto the next line with $($a + (1 * 2)) inside"
$escapes = "tab`t newline`n quote`" dollar`$ done"
$braced = ${env:ProgramFiles(x86)} + ${ spaced name }
$scoped = $global:Config + $script:count + $using:remote + $env:PATH
$auto = $_, $?, $^, $$, $args, $input, $PSItem, $Error[0], $LastExitCode
$bool = $true -and $false -or -not $null
$splatted = @parameters
$broken = @"notAHereString"
$at = @
$paren = @(1, 2) + @{ key = 'value' }

$here = @"
First line with $variable and $($object.Property)
  "@ is not a terminator when indented
Nested $( if ($x) { "inner $y" } else { 'other' } ) text
"@
$verbatim = @'
Keep $this and $(that) as they are
'@

$nums = 0xFF, 1kb, 1.5GB, 10l, 3d, 1e10, .5, 5., 7MB, 2pb
$ops = 1 -eq 1 -ieq 2 -ceq 3 -ne 4 -gt 5 -ge 6 -lt 7 -le 8
$like = 'a' -like 'A*' -notlike 'b' -match '^a' -notmatch 'z' -cmatch 'a'
$sets = 1 -in @(1) -notin @(2) -contains 3 -notcontains 4
$text = 'a,b' -split ',' -isplit ';' -csplit ':' -join '+' -replace 'a', 'b' -creplace 'c'
$bits = 5 -band 3 -bor 8 -bxor 1 -bnot 2 -shl 1 -shr 1
$types = $x -is [string] -isnot [int] -as [double]
$fmt = '{0:N2}' -f 3.14159
$counter += 1; $counter -= 1; $counter *= 2; $counter /= 2; $counter %= 3
$counter++; $counter--; $range = 1..5; $not = !$true
Get-Item foo 2>&1 > out.txt >> log.txt
$result = & { param($a) $a } -a 1

filter Select-Even { if ($_ % 2 -eq 0) { $_ } }
function Test-Stages {
    [CmdletBinding()]
    param()
    dynamicparam { }
    begin { $total = 0 }
    process { $total += $_ }
    end { $total }
}
trap { Write-Warning $_; continue }
data { 'static data' }

class Person {
    [string] $Name
    [int] $Age = 0
    Person([string] $name) { $this.Name = $name }
    [string] ToString() { return "$($this.Name) ($($this.Age))" }
}
enum Color { Red; Green = 2; Blue }
using namespace System.Text

$path = [System.IO.Path]::Combine('C:\', 'temp')
$array = [int[]] @(1, 2, 3)
$builder = New-Object -TypeName System.Text.StringBuilder
[void] $builder.Append("x")
Get-ChildItem -Path C: | Get-Unknown-Thing -Flag
$drive = D:
$long = Get-Content -Path $path `
    -Encoding UTF8 `
    -Raw
if ($long.Length -gt 0) { "non-empty" } elseif ($null -eq $long) { 'null' }
$emptyBrace = ${}
$lonely = $
#endregion
