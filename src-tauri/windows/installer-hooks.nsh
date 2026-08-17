; Mnemark installer compatibility hooks.
;
; The product was renamed from ClipFlow to Mnemark. NSIS keys installation
; state by productName, so a stock Mnemark installer would not see the old
; ClipFlow install and would install side-by-side. These hooks migrate it:
;   PREINSTALL  — kill any running Mnemark/ClipFlow, detect the legacy
;                 ClipFlow uninstall record (HKCU then HKLM), run its
;                 uninstaller silently (preserving user data), and abort with
;                 a manual-remediation message if removal fails or the old
;                 identity survives.
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
  StrCpy $MnemarkMigrateDesktopShortcut 0
  StrCpy $MnemarkMigrateStartMenuShortcut 0
  StrCpy $R0 ""
  StrCpy $R1 ""
  ReadRegStr $R0 HKCU "${LEGACY_UNINSTKEY}" "UninstallString"
  ReadRegStr $R1 HKCU "Software\${MANUFACTURER}\${LEGACY_PRODUCTNAME}" ""
  StrCmp $R0 "" 0 legacy_found
  ReadRegStr $R0 HKLM "${LEGACY_UNINSTKEY}" "UninstallString"
  ReadRegStr $R1 HKLM "Software\${MANUFACTURER}\${LEGACY_PRODUCTNAME}" ""
  StrCmp $R0 "" 0 legacy_found
  Goto legacy_none

  legacy_found:
    ; Record shortcut presence before the old uninstaller removes them.
    ${If} ${FileExists} "$DESKTOP\${LEGACY_PRODUCTNAME}.lnk"
      StrCpy $MnemarkMigrateDesktopShortcut 1
    ${EndIf}
    ${If} ${FileExists} "$SMPROGRAMS\${LEGACY_PRODUCTNAME}.lnk"
      StrCpy $MnemarkMigrateStartMenuShortcut 1
    ${EndIf}

    ; Run the old uninstaller silently. The app-data delete checkbox defaults
    ; to off and is only read after the confirm page, which silent mode skips,
    ; so user data in %APPDATA% is preserved.
    DetailPrint "Removing legacy ClipFlow installation..."
    StrCpy $R0 "$R0 /S _?=$R1"
    ExecWait '$R0' $R2

    ; The old uninstaller must have succeeded and removed its identity. If it
    ; failed, or the ClipFlow uninstall record survives, do not install
    ; side-by-side — surface a manual-remediation message instead.
    ReadRegStr $R3 HKCU "${LEGACY_UNINSTKEY}" "UninstallString"
    ReadRegStr $R4 HKLM "${LEGACY_UNINSTKEY}" "UninstallString"
    ${If} $R2 != 0
    ${OrIf} $R3 != ""
    ${OrIf} $R4 != ""
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
