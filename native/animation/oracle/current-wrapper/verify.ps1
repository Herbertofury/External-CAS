param([Parameter(Mandatory=$true)][string]$PrimaryReaderSrc)
$ErrorActionPreference='Stop'
$pins=@{'ViewerV2/StudioClipDecoderV2.cs'='eed5396adca6a9f880d643e17ee27cc2690cdbc848454531236c4a27dbdc7322';'Animation/ClipAnimation.cs'='33fb7524a10a830462580ea864b1d3b42edba76e2bb62050df4a3e8bf7485f00';'Animation/ClipChannelType.cs'='779fbe3f429e20c9f4cad654eec597588c098e4233572fa186cc394c21e2e875'}
foreach($relative in $pins.Keys){if((Get-FileHash -LiteralPath (Join-Path $PrimaryReaderSrc $relative) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pins[$relative]){throw "Primary reader changed: $relative"}}
