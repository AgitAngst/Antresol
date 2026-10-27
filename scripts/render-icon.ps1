# Собирает PNG и antresol.ico из assets/icon/antresol.svg и antresol-small.svg (нужен resvg: scoop install resvg).
$ErrorActionPreference = 'Stop'
$dir = Join-Path (Split-Path -Parent $PSScriptRoot) 'assets\icon'
foreach ($n in 16, 24, 32, 48) { resvg -w $n -h $n (Join-Path $dir 'antresol-small.svg') (Join-Path $dir "antresol-$n.png") }
foreach ($n in 64, 128, 256, 512) { resvg -w $n -h $n (Join-Path $dir 'antresol.svg') (Join-Path $dir "antresol-$n.png") }
$sizes = 16, 24, 32, 48, 64, 128, 256
$frames = $sizes | ForEach-Object { , [System.IO.File]::ReadAllBytes((Join-Path $dir "antresol-$_.png")) }
$ms = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($ms)
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $n = $sizes[$i]; $len = $frames[$i].Length
    $w.Write([byte]($n % 256)); $w.Write([byte]($n % 256)); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32); $w.Write([uint32]$len); $w.Write([uint32]$offset)
    $offset += $len
}
foreach ($f in $frames) { $w.Write($f) }
[System.IO.File]::WriteAllBytes((Join-Path $dir 'antresol.ico'), $ms.ToArray())
