# Builds both halves of EldenCraft and packs a release into dist\ (run on Windows; CI runs it).
#   EldenCraft-<version>.zip            -> {managed}: eldencraft\eldencraft.dll + its ModEngine2 profile
#   EldenCraft-Minecraft-<version>.zip  -> {localappdata}\EldenCraft: portable Prism Launcher with the
#                                          "EldenCraft" instance (Minecraft 26.3, Fabric, Fabric API,
#                                          SkyCraft's Minecraft mod by chasmlol, unmodified)
# Logs of every step go to dist\logs\.
param([switch]$NoBuild)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$root = Split-Path -Parent $PSScriptRoot
$version = (Select-String -Path "$root\er\Cargo.toml" -Pattern '^version = "(.+)"$').Matches[0].Groups[1].Value

# Pinned inputs (hashes checked).
$skycraftRepo = "https://github.com/chasmlol/SkyCraft"
$skycraftCommit = "bfcaf178524b92c2cdeb88e4ce0f13ef9ded6f32"
$prismVersion = "11.1.1"
$prismZip = "PrismLauncher-Windows-MSVC-Portable-$prismVersion.zip"
$prismUrl = "https://github.com/PrismLauncher/PrismLauncher/releases/download/$prismVersion/$prismZip"
$prismSha256 = "ab35a770fb06d89d2ccc098079db5db329fb4e68f42b72babd8b095efde3d2d7"
$prismLicenseUrl = "https://raw.githubusercontent.com/PrismLauncher/PrismLauncher/$prismVersion/LICENSE"
# ModEngine2 (MIT), bundled: it starts Elden Ring offline with Easy Anti-Cheat off and loads eldencraft.dll.
$me2Zip = "ModEngine-2.1.0.0-win64.zip"
$me2Url = "https://github.com/soulsmods/ModEngine2/releases/download/release-2.1.0/$me2Zip"
$me2Sha256 = "8a5952453a8e3851247ed6a9dd8926ca638cf6135aa96c90298b63f15d8900ef"
$me2LicenseUrl = "https://raw.githubusercontent.com/soulsmods/ModEngine2/main/LICENSE-MIT"
$fabricApiJar = "fabric-api-0.161.0+26.3.jar"
$fabricApiUrl = "https://cdn.modrinth.com/data/P7dR8mSH/versions/bNnaTiuM/fabric-api-0.161.0%2B26.3.jar"
$fabricApiSha512 = "ed6b2586d6fde11fde8472f5a527c51e99b67026e46f94d4bfd85e7e28ce5ee299173ee16ad576ceb51f39f98d30a811086a6deb1a86a524859cc16e12da109d"

$dist = "$root\dist"
$logs = "$dist\logs"
New-Item -ItemType Directory $logs -Force | Out-Null

function Get-Pinned([string]$url, [string]$path, [string]$algorithm, [string]$hash) {
    if (-not (Test-Path $path)) {
        New-Item -ItemType Directory (Split-Path $path) -Force | Out-Null
        Invoke-WebRequest -Uri $url -OutFile $path -UseBasicParsing
    }
    if ($hash -and (Get-FileHash $path -Algorithm $algorithm).Hash -ne $hash.ToUpper()) {
        Remove-Item $path
        throw "$path doesn't match its pinned $algorithm hash"
    }
}

Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
function New-Zip([string]$path, [System.Collections.IDictionary]$entries) {
    $zip = [System.IO.Compression.ZipFile]::Open($path, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($name in $entries.Keys) {
            [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $entries[$name], $name, [System.IO.Compression.CompressionLevel]::Optimal) | Out-Null
        }
    } finally { $zip.Dispose() }
}
function New-ZipFromFolder([string]$path, [string]$folder) {
    $entries = [ordered]@{}
    $base = (Resolve-Path $folder).Path.TrimEnd('\') + '\'
    Get-ChildItem $folder -Recurse -File | Sort-Object FullName | ForEach-Object {
        $entries[$_.FullName.Substring($base.Length).Replace('\', '/')] = $_.FullName
    }
    New-Zip $path $entries
}
function Invoke-Logged([string]$name, [scriptblock]$block) {
    & $block 2>&1 | Tee-Object -FilePath "$logs\$name.log"
    if ($LASTEXITCODE) { throw "$name failed (exit $LASTEXITCODE); see logs\$name.log" }
}

$skySrc = "$root\.tools\skycraft"
if (-not $NoBuild) {
    Invoke-Logged "er-build" { cargo build --release --manifest-path "$root\er\Cargo.toml" }
    if (-not (Test-Path "$skySrc\.git")) {
        Invoke-Logged "skycraft-clone" { git clone --quiet $skycraftRepo $skySrc }
    }
    Invoke-Logged "skycraft-checkout" { git -C $skySrc checkout --quiet $skycraftCommit }
    Push-Location "$skySrc\fabric"
    try {
        Invoke-Logged "fabric-build" { .\gradlew.bat build -x test --no-configuration-cache --no-daemon }
    } finally { Pop-Location }
}

$dll = "$root\er\target\release\eldencraft.dll"
$pdb = "$root\er\target\release\eldencraft.pdb"
$jar = Get-ChildItem "$skySrc\fabric\build\libs" -Filter "skycraft-*.jar" | Where-Object { $_.Name -notmatch "sources" } | Select-Object -First 1
if (-not (Test-Path $dll)) { throw "missing $dll" }
if (-not $jar) { throw "missing the SkyCraft Fabric jar" }

$cache = "$root\.tools\prism"
Get-Pinned $prismUrl "$cache\$prismZip" SHA256 $prismSha256
Get-Pinned $fabricApiUrl "$cache\$fabricApiJar" SHA512 $fabricApiSha512
Get-Pinned $prismLicenseUrl "$cache\PrismLauncher-$prismVersion-LICENSE.txt" "" ""
Get-Pinned $me2Url "$cache\$me2Zip" SHA256 $me2Sha256
Get-Pinned $me2LicenseUrl "$cache\ModEngine2-LICENSE-MIT.txt" "" ""
$me2 = "$root\.tools\me2"
if (Test-Path $me2) { Remove-Item -Recurse -Force $me2 }
Expand-Archive "$cache\$me2Zip" $me2 -Force
$me2Root = "$me2\ModEngine-2.1.0.0-win64"

Get-ChildItem $dist -Exclude logs | Remove-Item -Recurse -Force

# The bundled Minecraft.
$bundle = "$dist\bundle"
Copy-Item -Recurse "$root\minecraft-bundle" $bundle
Expand-Archive "$cache\$prismZip" "$bundle\Prism" -Force
Copy-Item "$cache\PrismLauncher-$prismVersion-LICENSE.txt" "$bundle\Prism\LICENSE-PrismLauncher.txt"
(Get-Content "$bundle\Prism\THIRD-PARTY.txt" -Raw).Replace("{PRISM_VERSION}", $prismVersion).Replace("{SKYCRAFT_COMMIT}", $skycraftCommit) | Set-Content "$bundle\Prism\THIRD-PARTY.txt" -NoNewline
$mods = "$bundle\Prism\instances\EldenCraft\.minecraft\mods"
New-Item -ItemType Directory $mods -Force | Out-Null
Copy-Item "$cache\$fabricApiJar" $mods
Copy-Item $jar.FullName "$mods\$($jar.Name)"
Copy-Item "$skySrc\LICENSE" "$bundle\Prism\instances\EldenCraft\.minecraft\mods\SkyCraft-LICENSE.txt"
Set-Content "$bundle\bundle-version.txt" "EldenCraft $version, Prism Launcher $prismVersion, $fabricApiJar, $($jar.Name) ($skycraftCommit)" -NoNewline
New-ZipFromFolder "$dist\EldenCraft-Minecraft-$version.zip" $bundle
Remove-Item -Recurse -Force $bundle

New-Zip "$dist\EldenCraft-$version.zip" ([ordered]@{
    "modengine2/modengine2_launcher.exe" = "$me2Root\modengine2_launcher.exe"
    "modengine2/modengine2/bin/modengine2.dll" = "$me2Root\modengine2\bin\modengine2.dll"
    "modengine2/modengine2/bin/lua.dll" = "$me2Root\modengine2\bin\lua.dll"
    "modengine2/modengine2/crashpad/crashpad_handler.exe" = "$me2Root\modengine2\crashpad\crashpad_handler.exe"
    "modengine2/modengine2/crashpad/zlib1.dll" = "$me2Root\modengine2\crashpad\zlib1.dll"
    "modengine2/README.txt" = "$me2Root\README.txt"
    "modengine2/LICENSE-ModEngine2.txt" = "$cache\ModEngine2-LICENSE-MIT.txt"
    "eldencraft/eldencraft.dll" = $dll
    "eldencraft/config_eldencraft.toml" = "$root\er\config_eldencraft.toml"
    "eldencraft/LICENSE.txt" = "$root\LICENSE"
    "eldencraft/THIRD-PARTY-NOTICES.md" = "$root\THIRD-PARTY-NOTICES.md"
})
if (Test-Path $pdb) { New-Zip "$dist\EldenCraft-$version-pdb.zip" ([ordered]@{ "eldencraft.pdb" = $pdb }) }

Get-ChildItem $dist -File | ForEach-Object {
    "{0,-44} {1,12:N0} bytes  sha256 {2}" -f $_.Name, $_.Length, (Get-FileHash $_.FullName -Algorithm SHA256).Hash
} | Tee-Object -FilePath "$logs\dist.txt"
