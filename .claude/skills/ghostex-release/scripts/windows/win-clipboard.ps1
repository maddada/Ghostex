# Release smoke (Windows): save, set and restore the user's clipboard around the 4.1 paste test.
#   win-clipboard.ps1 save <folder> | set <text> | restore <folder> | hash
# `save` keeps text, HTML, RTF, file lists and images (PNG stream, else the bitmap) in <folder>;
# `restore` puts back what was saved; `hash` prints a digest of what is on the clipboard now,
# so `hash` before `save` and after `restore` proves the restore. Runs in an STA Windows
# PowerShell because the WinForms clipboard needs one.
param([Parameter(Mandatory)][ValidateSet('save','set','restore','hash')][string]$Verb, [string]$Arg)
$body = @'
param($Verb, $Arg)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$C = [System.Windows.Forms.Clipboard]
function Get-Digest {
  $d = $C::GetDataObject(); if (-not $d) { return 'empty' }
  $parts = foreach ($f in ($d.GetFormats() | Sort-Object)) {
    $v = $null; try { $v = $d.GetData($f) } catch {}
    if ($v -is [string]) { "$f=" + $v }
    elseif ($v -is [System.IO.MemoryStream]) { "$f=" + [Convert]::ToBase64String($v.ToArray()) }
    else { "$f" }
  }
  $sha = [Security.Cryptography.SHA256]::Create()
  [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($parts -join "`n")))).Replace('-','').Substring(0,16) + " (" + ($d.GetFormats() -join ',') + ")"
}
switch ($Verb) {
  'hash' { Get-Digest }
  'set' { $C::SetText($Arg); 'set' }
  'save' {
    New-Item -ItemType Directory -Force $Arg | Out-Null
    Get-ChildItem $Arg -File | Remove-Item
    $d = $C::GetDataObject()
    if ($C::ContainsText()) { [IO.File]::WriteAllText("$Arg\text.txt", $C::GetText()) }
    if ($d.GetDataPresent('HTML Format')) { [IO.File]::WriteAllText("$Arg\html.txt", [string]$d.GetData('HTML Format')) }
    if ($d.GetDataPresent('Rich Text Format')) { [IO.File]::WriteAllText("$Arg\rtf.txt", [string]$d.GetData('Rich Text Format')) }
    if ($C::ContainsFileDropList()) { $C::GetFileDropList() | Set-Content "$Arg\files.txt" }
    if ($d.GetDataPresent('PNG')) { [IO.File]::WriteAllBytes("$Arg\image.png", $d.GetData('PNG').ToArray()) }
    elseif ($C::ContainsImage()) { $C::GetImage().Save("$Arg\image.png", [System.Drawing.Imaging.ImageFormat]::Png) }
    'saved: ' + ((Get-ChildItem $Arg -File).Name -join ', ')
  }
  'restore' {
    $o = New-Object System.Windows.Forms.DataObject
    if (Test-Path "$Arg\text.txt") { $o.SetText([IO.File]::ReadAllText("$Arg\text.txt")) }
    if (Test-Path "$Arg\html.txt") { $o.SetData('HTML Format', [IO.File]::ReadAllText("$Arg\html.txt")) }
    if (Test-Path "$Arg\rtf.txt") { $o.SetData('Rich Text Format', [IO.File]::ReadAllText("$Arg\rtf.txt")) }
    if (Test-Path "$Arg\files.txt") { $l = New-Object System.Collections.Specialized.StringCollection; Get-Content "$Arg\files.txt" | ForEach-Object { [void]$l.Add($_) }; $o.SetFileDropList($l) }
    if (Test-Path "$Arg\image.png") {
      $bytes = [IO.File]::ReadAllBytes("$Arg\image.png")
      $o.SetData('PNG', (New-Object IO.MemoryStream(,$bytes)))
      $o.SetImage([System.Drawing.Image]::FromStream((New-Object IO.MemoryStream(,$bytes))))
    }
    if (-not (Get-ChildItem $Arg -File)) { $C::Clear() } else { $C::SetDataObject($o, $true) }
    'restored'
  }
}
'@
$tmp = Join-Path $env:TEMP 'gx-smoke-clipboard.ps1'
Set-Content -Path $tmp -Value $body -Encoding UTF8
powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File $tmp $Verb $Arg
