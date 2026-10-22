# Пережать PNG без потерь (GDI+): снимки `ANTRESOL_SHOT` пишутся без сжатия и весят мегабайты.
# Использование: powershell -NoProfile -File scripts\squeeze-png.ps1 -In снимок.png -Out docs\img\имя.png
param([Parameter(Mandatory)][string]$In, [Parameter(Mandatory)][string]$Out)
Add-Type -AssemblyName System.Drawing
$src = [System.Drawing.Bitmap]::FromFile((Resolve-Path $In))
# Копия в памяти: исходный файл отпускаем до записи (можно писать поверх него же).
$bmp = New-Object System.Drawing.Bitmap $src
$src.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
"{0}: {1:N0} KB" -f $Out, ((Get-Item $Out).Length / 1KB)
