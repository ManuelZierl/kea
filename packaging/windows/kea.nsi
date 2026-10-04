Unicode true
!include "MUI2.nsh"

!ifndef VERSION
  !error "VERSION must be defined"
!endif
!ifndef SOURCE_EXE
  !error "SOURCE_EXE must be defined"
!endif
!ifndef LICENSE_FILE
  !error "LICENSE_FILE must be defined"
!endif
!ifndef ICON_FILE
  !error "ICON_FILE must be defined"
!endif
!ifndef OUTFILE
  !error "OUTFILE must be defined"
!endif

Name "Kea"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Kea"
RequestExecutionLevel user
Icon "${ICON_FILE}"
UninstallIcon "${ICON_FILE}"
SetCompressor /SOLID lzma

!define MUI_ABORTWARNING
!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Kea" SEC_MAIN
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "/oname=kea.exe" "${SOURCE_EXE}"
  File "/oname=LICENSE" "${LICENSE_FILE}"

  CreateDirectory "$SMPROGRAMS\Kea"
  CreateShortcut "$SMPROGRAMS\Kea\Kea.lnk" "$INSTDIR\kea.exe" "" "$INSTDIR\kea.exe" 0

  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Kea" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea" "DisplayName" "Kea"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea" "Publisher" "Kea"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea" "DisplayIcon" "$INSTDIR\kea.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea" "URLInfoAbout" "https://github.com/ManuelZierl/kea"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$SMPROGRAMS\Kea\Kea.lnk"
  RMDir "$SMPROGRAMS\Kea"
  Delete "$INSTDIR\kea.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kea"
  DeleteRegKey HKCU "Software\Kea"
SectionEnd
