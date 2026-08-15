$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($env:PORTABLE_SOURCE)) {
  throw 'PORTABLE_SOURCE is required'
}

if ([string]::IsNullOrWhiteSpace($env:PORTABLE_DESTINATION)) {
  throw 'PORTABLE_DESTINATION is required'
}

Add-Type -AssemblyName System.IO.Compression.FileSystem

$sourcePath = ([IO.Path]::GetFullPath($env:PORTABLE_SOURCE)).TrimEnd([char[]]@('\', '/'))
$destinationPath = [IO.Path]::GetFullPath($env:PORTABLE_DESTINATION)
$rootName = [IO.Path]::GetFileName($sourcePath)
$archive = [IO.Compression.ZipFile]::Open(
  $destinationPath,
  [IO.Compression.ZipArchiveMode]::Create
)

try {
  $files = [IO.Directory]::EnumerateFiles(
    $sourcePath,
    '*',
    [IO.SearchOption]::AllDirectories
  ) | Sort-Object

  foreach ($filePath in $files) {
    $relativePath = $filePath.Substring($sourcePath.Length + 1).Replace(
      [IO.Path]::DirectorySeparatorChar,
      [IO.Path]::AltDirectorySeparatorChar
    )
    $entryName = "$rootName/$relativePath"
    [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
      $archive,
      $filePath,
      $entryName,
      [IO.Compression.CompressionLevel]::Optimal
    ) | Out-Null
  }
}
finally {
  $archive.Dispose()
}
