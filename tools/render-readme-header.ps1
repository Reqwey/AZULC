param(
    [string]$BackgroundPath = 'assets/brand/readme-header-background.png',
    [string]$OutputPath = 'assets/brand/readme-header.png'
)

Add-Type -AssemblyName System.Drawing

$projectRoot = Split-Path -Parent $PSScriptRoot
$background = [System.Drawing.Image]::FromFile((Join-Path $projectRoot $BackgroundPath))
$source = [System.Drawing.Bitmap]::FromFile((Join-Path $projectRoot 'assets/brand/girl-source.png'))
$displayFonts = New-Object System.Drawing.Text.PrivateFontCollection
$displayFonts.AddFontFile((Join-Path $projectRoot 'assets/fonts/PixelifySans-Variable.ttf'))
$bodyFonts = New-Object System.Drawing.Text.PrivateFontCollection
$bodyFonts.AddFontFile((Join-Path $projectRoot 'assets/fonts/SpaceMono-Regular.ttf'))
$bodyFonts.AddFontFile((Join-Path $projectRoot 'assets/fonts/SpaceMono-Bold.ttf'))

$width = 1600
$height = 520
$canvas = New-Object System.Drawing.Bitmap $width, $height, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$graphics = [System.Drawing.Graphics]::FromImage($canvas)
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit

try {
    $sourceRatio = $width / $height
    $cropHeight = [int]($background.Width / $sourceRatio)
    $cropY = [int](($background.Height - $cropHeight) / 2)
    $graphics.DrawImage(
        $background,
        (New-Object System.Drawing.Rectangle 0, 0, $width, $height),
        (New-Object System.Drawing.Rectangle 0, $cropY, $background.Width, $cropHeight),
        [System.Drawing.GraphicsUnit]::Pixel
    )
    $graphics.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(74, 16, 13, 25))), 0, 0, $width, $height)

    $border = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(180, 112, 88, 151)), 2
    $accent = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 184, 156, 255)), 4
    $graphics.DrawRectangle($border, 18, 18, $width - 37, $height - 37)
    $graphics.DrawLine($accent, 18, 18, 228, 18)
    $graphics.DrawLine($accent, $width - 228, $height - 19, $width - 18, $height - 19)
    $graphics.DrawLine($border, 110, 399, 1030, 399)

    $displayFamily = $displayFonts.Families[0]
    $bodyFamily = $bodyFonts.Families[0]
    $wordmarkFont = New-Object System.Drawing.Font $displayFamily, 148, ([System.Drawing.FontStyle]::Regular), ([System.Drawing.GraphicsUnit]::Pixel)
    $tagFont = New-Object System.Drawing.Font $bodyFamily, 24, ([System.Drawing.FontStyle]::Bold), ([System.Drawing.GraphicsUnit]::Pixel)
    $detailFont = New-Object System.Drawing.Font $bodyFamily, 17, ([System.Drawing.FontStyle]::Regular), ([System.Drawing.GraphicsUnit]::Pixel)

    $layers = @(
        @{ X = 132; Y = 126; Color = [System.Drawing.Color]::FromArgb(255, 16, 13, 25) },
        @{ X = 126; Y = 120; Color = [System.Drawing.Color]::FromArgb(255, 112, 104, 135) },
        @{ X = 120; Y = 114; Color = [System.Drawing.Color]::FromArgb(255, 184, 156, 255) },
        @{ X = 116; Y = 108; Color = [System.Drawing.Color]::FromArgb(255, 216, 200, 255) }
    )
    foreach ($layer in $layers) {
        $brush = New-Object System.Drawing.SolidBrush $layer.Color
        $graphics.DrawString('AZULC', $wordmarkFont, $brush, $layer.X, $layer.Y)
        $brush.Dispose()
    }

    $graphics.DrawString(
        'AZUSA MINECRAFT LAUNCHER',
        $tagFont,
        (New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 238, 232, 255))),
        120,
        314
    )
    $graphics.DrawString(
        'HIGH PERFORMANCE  /  LIGHTWEIGHT  /  CONCURRENCY SAFETY',
        $detailFont,
        (New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 184, 156, 255))),
        120,
        357
    )
    $graphics.DrawString(
        'NATIVE MINECRAFT LAUNCHER  //  TECHNOLOGY VALIDATION PLATFORM',
        $detailFont,
        (New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 177, 168, 196))),
        120,
        427
    )

    $tinted = New-Object System.Drawing.Bitmap $source.Width, $source.Height, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    for ($y = 0; $y -lt $source.Height; $y++) {
        for ($x = 0; $x -lt $source.Width; $x++) {
            $alpha = $source.GetPixel($x, $y).A
            if ($alpha -gt 0) {
                $tinted.SetPixel($x, $y, [System.Drawing.Color]::FromArgb($alpha, 216, 200, 255))
            }
        }
    }
    $graphics.DrawImage($tinted, (New-Object System.Drawing.Rectangle 1192, 55, 390, 404))
    $tinted.Dispose()

    $output = Join-Path $projectRoot $OutputPath
    $canvas.Save($output, [System.Drawing.Imaging.ImageFormat]::Png)
}
finally {
    $graphics.Dispose()
    $canvas.Dispose()
    $source.Dispose()
    $background.Dispose()
    $displayFonts.Dispose()
    $bodyFonts.Dispose()
}
