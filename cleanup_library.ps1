# Usage: .\cleanup_library.ps1 -libraryPath "C:\Path\To\Your\DAZ\Library"

param(
    [Parameter(Mandatory=$true, HelpMessage="Path to the DAZ library")]
    [ValidateScript({Test-Path $_ -PathType Container})]
    [string]$libraryPath
)

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "DAZ Library Cleanup" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Path: $libraryPath" -ForegroundColor Yellow
Write-Host ""

# Pre-cleanup analysis
Write-Host "Analyzing current state..." -ForegroundColor Green
$allFolders = Get-ChildItem $libraryPath -Directory -Recurse
$emptyFolders = $allFolders | Where-Object { (Get-ChildItem $_.FullName -Force).Count -eq 0 }
$totalFolders = $allFolders.Count
$emptyCount = $emptyFolders.Count

Write-Host "  Total folders: $totalFolders" -ForegroundColor White
Write-Host "  Empty folders: $emptyCount" -ForegroundColor Yellow

# Unwanted files
Write-Host ""
Write-Host "Searching for unwanted files..." -ForegroundColor Green

$unwantedFiles = Get-ChildItem $libraryPath -File -Recurse | Where-Object {
    $name = $_.Name.ToLower()
    $path = $_.FullName.ToLower()
    
    # Skip files inside DAZ standard folders
    $inDazFolder = $path -match '\\(data|people|runtime|environments|props|light presets|camera presets)\\'
    
    if ($inDazFolder) {
        return $false
    }
    
    # Unwanted patterns
    $name -match '(readme|license|promo|thumbs\.db|\.ds_store)' -or
    $name -match '\.(txt|pdf|html|htm|url)$' -or
    ($_.Directory.Name -match 'promo' -and $name -match '\.(png|jpg|jpeg)$') -or
    ($_.Directory.Parent.FullName -eq $libraryPath -and $name -match '\.(png|jpg|jpeg)$')
}

$unwantedCount = ($unwantedFiles | Measure-Object).Count
$unwantedSize = ($unwantedFiles | Measure-Object -Property Length -Sum).Sum

Write-Host "  Unwanted files: $unwantedCount" -ForegroundColor Yellow
Write-Host "  Size: $([math]::Round($unwantedSize / 1MB, 2)) MB" -ForegroundColor Yellow

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Summary" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Folders to delete: $emptyCount" -ForegroundColor Red
Write-Host "Files to delete: $unwantedCount" -ForegroundColor Red
Write-Host "Space to recover: $([math]::Round($unwantedSize / 1MB, 2)) MB" -ForegroundColor Red

Write-Host ""
$response = Read-Host "Proceed with cleanup? (Y/N)"

if ($response -eq "Y" -or $response -eq "y") {
    Write-Host ""
    Write-Host "Cleanup in progress..." -ForegroundColor Green
    
    # Delete unwanted files
    $deletedFiles = 0
    Write-Host "  Deleting files..." -ForegroundColor Yellow
    foreach ($file in $unwantedFiles) {
        try {
            Remove-Item $file.FullName -Force
            $deletedFiles++
            if ($deletedFiles % 10 -eq 0) {
                Write-Host "    Deleted: $deletedFiles/$unwantedCount" -ForegroundColor Gray
            }
        } catch {
            Write-Host "    Error: $($file.Name) - $_" -ForegroundColor Red
        }
    }
    
    # Delete empty folders (multiple passes)
    $deletedFolders = 0
    $pass = 1
    Write-Host ""
    Write-Host "  Deleting empty folders..." -ForegroundColor Yellow
    
    do {
        $deleted = 0
        $folders = Get-ChildItem $libraryPath -Directory -Recurse | 
            Sort-Object { $_.FullName.Length } -Descending
        
        foreach ($folder in $folders) {
            if ((Get-ChildItem $folder.FullName -Force).Count -eq 0) {
                try {
                    Remove-Item $folder.FullName -Force
                    $deleted++
                    $deletedFolders++
                } catch {
                    # Ignore errors
                }
            }
        }
        
        Write-Host ("    Pass {0}: {1} folders deleted" -f $pass, $deleted) -ForegroundColor Gray
        $pass++
    } while ($deleted -gt 0 -and $pass -le 10)
    
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host "Cleanup complete!" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host "Files deleted: $deletedFiles" -ForegroundColor Green
    Write-Host "Folders deleted: $deletedFolders" -ForegroundColor Green
    Write-Host "Space recovered: $([math]::Round($unwantedSize / 1MB, 2)) MB" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "Cleanup canceled." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Press any key to continue..."
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
