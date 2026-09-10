!include "MUI2.nsh"
!include "FileFunc.nsh"

SetCompressor /SOLID lzma

; General
Name "Clipped"
OutFile "releases\download\clipped_0.5.0_x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Clipped"
InstallDirRegKey HKCU "Software\Alexis\Clipped" "InstallLocation"
RequestExecutionLevel user
Unicode true

; Version Info
VIProductVersion "0.5.0.0"
VIAddVersionKey "ProductName" "Clipped"
VIAddVersionKey "CompanyName" "Alexis"
VIAddVersionKey "LegalCopyright" "Alexis"
VIAddVersionKey "FileDescription" "Clipped Installer"
VIAddVersionKey "FileVersion" "0.5.0.0"
VIAddVersionKey "ProductVersion" "0.5.0"

; Interface Settings
!define MUI_ABORTWARNING
!define MUI_ICON "assets\icon.ico"
!define MUI_UNICON "assets\icon.ico"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

; Finish page with option to run
!define MUI_FINISHPAGE_RUN "$INSTDIR\clipped.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Clipped"
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Languages
!insertmacro MUI_LANGUAGE "English"

; Installer Section
Section "Clipped" SecClipped
    DetailPrint "Closing running Clipped instances..."
    nsExec::Exec 'taskkill /F /IM clipped.exe'

    SetOutPath "$INSTDIR"
    File "target\release\clipped.exe"
    File "README.md"
    File "assets\icon.ico"

    ; Store installation folder
    WriteRegStr HKCU "Software\Alexis\Clipped" "InstallLocation" "$INSTDIR"

    ; Create uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Create Start Menu shortcut
    CreateDirectory "$SMPROGRAMS\Clipped"
    CreateShortcut "$SMPROGRAMS\Clipped\Clipped.lnk" "$INSTDIR\clipped.exe" "" "$INSTDIR\icon.ico" 0
    CreateShortcut "$SMPROGRAMS\Clipped\Uninstall.lnk" "$INSTDIR\uninstall.exe"

    ; Create Desktop shortcut
    CreateShortcut "$DESKTOP\Clipped.lnk" "$INSTDIR\clipped.exe" "" "$INSTDIR\icon.ico" 0

    ; Write Add/Remove Programs registry entries
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "DisplayName" "Clipped"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "DisplayVersion" "0.5.0"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "Publisher" "Alexis"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "DisplayIcon" "$INSTDIR\icon.ico"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "HelpLink" "https://github.com/alexeikmun/Clipped"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "URLInfoAbout" "https://github.com/alexeikmun/Clipped"
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "NoRepair" 1

    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped" "EstimatedSize" "$0"
SectionEnd

; Uninstaller Section
Section "Uninstall"
    DetailPrint "Closing running Clipped instances..."
    nsExec::Exec 'taskkill /F /IM clipped.exe'

    ; Remove files
    Delete "$INSTDIR\clipped.exe"
    Delete "$INSTDIR\README.md"
    Delete "$INSTDIR\icon.ico"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    ; Remove shortcuts
    Delete "$SMPROGRAMS\Clipped\Clipped.lnk"
    Delete "$SMPROGRAMS\Clipped\Uninstall.lnk"
    RMDir "$SMPROGRAMS\Clipped"
    Delete "$DESKTOP\Clipped.lnk"

    ; Remove registry entries
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clipped"
    DeleteRegKey HKCU "Software\Alexis\Clipped"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Clipped"
SectionEnd

