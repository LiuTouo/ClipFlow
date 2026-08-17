; Mnemark installer compatibility hooks.
;
; The product was renamed from ClipFlow to Mnemark. NSIS keys installation
; state by productName, so a stock Mnemark installer would not see the old
; ClipFlow install and would install side-by-side. These hooks migrate it:
;   PREINSTALL  — kill any running Mnemark/ClipFlow, detect the legacy
;                 ClipFlow uninstall record (HKCU then HKLM, reading
;                 UninstallString and InstallLocation from the same key), run
;                 its uninstaller passively (preserving user data), and abort
;                 with a manual-remediation message if removal fails or the
;                 old identity survives.
;   POSTINSTALL — recreate the Start Menu and Desktop shortcuts the legacy
;                 install had, as Mnemark equivalents (including /UPDATE mode,
;                 where the stock shortcut flow returns early).
; Legacy ClipFlow strings are confined to this compatibility layer by design.

!define LEGACY_PRODUCTNAME "ClipFlow"
!define LEGACY_EXE "clipflow.exe"
!define LEGACY_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipFlow"

; 1 = the legacy install had that shortcut; recreate an equivalent Mnemark
; shortcut after install. Declared as Vars so they survive between PREINSTALL
; and POSTINSTALL.
Var MnemarkMigrateDesktopShortcut
Var MnemarkMigrateStartMenuShortcut

!macro NSIS_HOOK_PREINSTALL
  ; Close any running Mnemark or legacy ClipFlow before touching files.
  DetailPrint "Closing running Mnemark..."
  nsExec::ExecToStack 'taskkill.exe /F /T /IM ${MAINBINARYNAME}.exe'
  Pop $0 ; exit code (ignored — non-zero just means nothing was running)
  Pop $1 ; stdout
  DetailPrint "Closing running ClipFlow..."
  nsExec::ExecToStack 'taskkill.exe /F /T /IM ${LEGACY_EXE}'
  Pop $0
  Pop $1
  Sleep 500

  ; Detect the legacy ClipFlow uninstall record: per-user first, then machine.
  ; Read UninstallString AND InstallLocation from the SAME uninstall key and
  ; hive, tracking which hive held the record. v0.5.7 Tauri writes
  ; InstallLocation (quoted) on this key; reading it from a separate
  ; manufacturer/product key returns empty and yields _?= (the v0.6.0 bug).
  StrCpy $MnemarkMigrateDesktopShortcut 0
  StrCpy $MnemarkMigrateStartMenuShortcut 0
  StrCpy $R0 ""
  StrCpy $R1 ""
  StrCpy $R5 ""
  ReadRegStr $R0 HKCU "${LEGACY_UNINSTKEY}" "UninstallString"
  ReadRegStr $R1 HKCU "${LEGACY_UNINSTKEY}" "InstallLocation"
  StrCmp $R0 "" 0 legacy_hkcu_found
  ReadRegStr $R0 HKLM "${LEGACY_UNINSTKEY}" "UninstallString"
  ReadRegStr $R1 HKLM "${LEGACY_UNINSTKEY}" "InstallLocation"
  StrCmp $R0 "" 0 legacy_hklm_found
  Goto legacy_none

  legacy_hkcu_found:
    StrCpy $R5 "HKCU"
    Goto legacy_found
  legacy_hklm_found:
    StrCpy $R5 "HKLM"
    Goto legacy_found

  legacy_found:
    ; Record shortcut presence before the old uninstaller removes them.
    ${If} ${FileExists} "$DESKTOP\${LEGACY_PRODUCTNAME}.lnk"
      StrCpy $MnemarkMigrateDesktopShortcut 1
    ${EndIf}
    ${If} ${FileExists} "$SMPROGRAMS\${LEGACY_PRODUCTNAME}.lnk"
      StrCpy $MnemarkMigrateStartMenuShortcut 1
    ${EndIf}

    ; Normalize the two registry values: Tauri stores both quoted. The _?=
    ; switch must be passed an unquoted directory, even when it contains spaces.
    ${WordReplace} $R0 '"' '' '+' $R0
    ${WordReplace} $R1 '"' '' '+' $R1
    ; If InstallLocation is missing, derive the directory from the uninstaller
    ; path (e.g. C:\Program Files\ClipFlow\uninstall.exe).
    ${If} $R1 == ""
      ${GetParent} $R0 $R1
    ${EndIf}

    ; Never run the old uninstaller with an empty _?=: that would skip removal
    ; and install side-by-side. Abort with a remediation message instead.
    ${If} $R1 == ""
      DetailPrint "Legacy ClipFlow install directory is empty (UninstallString='$R0', hive=$R5); cannot uninstall safely."
      Abort "Mnemark could not determine the old ClipFlow install location. Uninstall ClipFlow from Add/Remove Programs, then run this installer again."
    ${EndIf}

    ; Run the old uninstaller passively (/P + _?=<install-dir>), matching
    ; Tauri's stock NSIS uninstall semantics. Passive mode skips the confirm
    ; page that gates the app-data delete checkbox, so user data in %APPDATA%
    ; is preserved.
    DetailPrint "Removing legacy ClipFlow installation (hive=$R5)..."
    StrCpy $R0 '"$R0" /P _?=$R1'
    ClearErrors
    ExecWait '$R0' $R2
    ${If} ${Errors}
      DetailPrint "Legacy ClipFlow uninstaller could not be launched: $R0"
      Abort "Mnemark could not launch the old ClipFlow uninstaller. Uninstall ClipFlow from Add/Remove Programs, then run this installer again."
    ${EndIf}
    ${If} $R2 != 0
      DetailPrint "Legacy ClipFlow uninstaller exited with code $R2."
      Abort "Mnemark could not remove the old ClipFlow installation automatically. Uninstall ClipFlow from Add/Remove Programs, then run this installer again."
    ${EndIf}

    ; The old uninstaller must have removed its identity. Re-check only the
    ; hive we selected, so an unrelated per-user/machine install does not
    ; produce a false positive and abort a valid side.
    ${If} $R5 == "HKCU"
      ReadRegStr $R3 HKCU "${LEGACY_UNINSTKEY}" "UninstallString"
    ${Else}
      ReadRegStr $R3 HKLM "${LEGACY_UNINSTKEY}" "UninstallString"
    ${EndIf}
    ${If} $R3 != ""
      Abort "Mnemark could not remove the old ClipFlow installation automatically. Uninstall ClipFlow from Add/Remove Programs, then run this installer again."
    ${EndIf}

  legacy_none:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Preserve legacy shortcut presence with equivalent Mnemark shortcuts. This
  ; runs after the stock shortcut flow, which returns early in /UPDATE mode, so
  ; an in-app updater that launched this installer with /UPDATE still recreates
  ; the Start Menu shortcut the legacy install had. Never create a shortcut the
  ; legacy install did not have.
  ${If} $MnemarkMigrateStartMenuShortcut = 1
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  ${EndIf}
  ${If} $MnemarkMigrateDesktopShortcut = 1
    CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Closing running Mnemark..."
  nsExec::ExecToStack 'taskkill.exe /F /T /IM ${MAINBINARYNAME}.exe'
  Pop $0
  Pop $1
  Sleep 500
!macroend
