; Windows installer of the com CLI (scripts/build-installers.ts): per user, no admin rights, adds the
; install folder to the user's PATH. makensis -DVERSION=<v> -DSRC=<folder> -DOUT=<setup.exe> com.nsi
Unicode true
!include "MUI2.nsh"

Name "Com.AI.VN CLI ${VERSION}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\com"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "Com.AI.VN"

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\ComAiVnCli"
!define PATH_SCRIPT "$$u=[Environment]::GetEnvironmentVariable('Path','User'); if(-not $$u){$$u=''}; $$parts=@($$u -split ';' | Where-Object { $$_ -and $$_ -ne $$d })"

!define MUI_FINISHPAGE_TEXT "com is installed. Open a new terminal and run: com create my-app"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  ; An upgrade replaces the CLI, never mixes two versions; lib\, runtime\ and com.cmd are the layout
  ; before the standalone com.exe
  RMDir /r "$INSTDIR\lib"
  RMDir /r "$INSTDIR\runtime"
  Delete "$INSTDIR\com.cmd"
  SetOutPath "$INSTDIR"
  File /r "${SRC}\*"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  nsExec::ExecToLog `powershell -NoProfile -ExecutionPolicy Bypass -Command "$$d='$INSTDIR'; ${PATH_SCRIPT}; [Environment]::SetEnvironmentVariable('Path', (@($$parts) + $$d) -join ';', 'User')"`

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Com.AI.VN CLI"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Com.AI.VN"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  nsExec::ExecToLog `powershell -NoProfile -ExecutionPolicy Bypass -Command "$$d='$INSTDIR'; ${PATH_SCRIPT}; [Environment]::SetEnvironmentVariable('Path', @($$parts) -join ';', 'User')"`
  RMDir /r "$INSTDIR"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
