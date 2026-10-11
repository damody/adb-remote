!include "MUI2.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif

!define PRODUCT_NAME "ADB Remote"
!define COMPANY_NAME "ADB Remote"
!define INSTALL_DIR "$PROGRAMFILES64\ADB Remote"
!ifndef SOURCE_ROOT
  !define SOURCE_ROOT "${__FILEDIR__}\.."
!endif

Name "${PRODUCT_NAME}"
OutFile "${SOURCE_ROOT}\dist\adb-remote-${VERSION}-windows-x64-setup.exe"
InstallDir "${INSTALL_DIR}"
InstallDirRegKey HKLM "Software\${COMPANY_NAME}\${PRODUCT_NAME}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
Unicode true

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "CompanyName" "${COMPANY_NAME}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} Installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright 2026 ${COMPANY_NAME}"

!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

Section "ADB Remote" SecMain
  SetOutPath "$INSTDIR"

  File "${SOURCE_ROOT}\target\release\adb-remote.exe"
  File "${SOURCE_ROOT}\target\release\adb-remote-client.exe"
  File "${SOURCE_ROOT}\target\release\adb-remote-server.exe"
  File "${SOURCE_ROOT}\docs\mvp-usage.md"
  File "${SOURCE_ROOT}\docs\server-linux.md"
  File "${SOURCE_ROOT}\scripts\manual-adb-test.ps1"

  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\${COMPANY_NAME}\${PRODUCT_NAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" "Publisher" "${COMPANY_NAME}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" "NoRepair" 1

  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\ADB Remote Docs.lnk" "$INSTDIR\mvp-usage.md"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Install Directory.lnk" "$INSTDIR"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\adb-remote.exe"
  Delete "$INSTDIR\adb-remote-client.exe"
  Delete "$INSTDIR\adb-remote-server.exe"
  Delete "$INSTDIR\mvp-usage.md"
  Delete "$INSTDIR\server-linux.md"
  Delete "$INSTDIR\manual-adb-test.ps1"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${PRODUCT_NAME}\ADB Remote Docs.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Install Directory.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
  DeleteRegKey HKLM "Software\${COMPANY_NAME}\${PRODUCT_NAME}"
SectionEnd
