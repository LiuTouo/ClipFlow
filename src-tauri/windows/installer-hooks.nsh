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

; Single source of truth for the rebrand download URL: shown verbatim in the
; auto-update notice AND opened via ExecShell on request. Reuse this define so
; the two can never drift apart.
!define REBRAND_DOWNLOAD_URL "https://github.com/LiuTouo/Mnemark/releases/latest"

; 1 = the legacy install had that shortcut; recreate an equivalent Mnemark
; shortcut after install. Declared as Vars so they survive between PREINSTALL
; and POSTINSTALL.
Var MnemarkMigrateDesktopShortcut
Var MnemarkMigrateStartMenuShortcut
; MnemarkLegacyFound: 1 once a legacy ClipFlow uninstall record is detected.
; MnemarkRebrandAbort: 1 when the guard decides the auto-update must abort.
Var MnemarkLegacyFound
Var MnemarkRebrandAbort

; Pure decision for the cross-brand auto-update guard. Sets
; MnemarkRebrandAbort to 1 only when BOTH the Tauri /UPDATE mode is active AND
; a legacy ClipFlow uninstall record was detected. Kept as a standalone macro
; so the proof fixture can drive it headlessly without a MessageBox/registry.
!macro REBRAND_GUARD_DECIDE
  StrCpy $MnemarkRebrandAbort 0
  ${If} $UpdateMode = 1
  ${AndIf} $MnemarkLegacyFound = 1
    StrCpy $MnemarkRebrandAbort 1
  ${EndIf}
!macroend

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
  StrCpy $MnemarkLegacyFound 0
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
    StrCpy $MnemarkLegacyFound 1
    !insertmacro REBRAND_GUARD_DECIDE

    ; Background updater (/UPDATE) must not auto-migrate a legacy ClipFlow
    ; install: the cross-brand uninstall would silently remove the old app.
    ; Show an explicit rebrand notice (with the download URL) and abort before
    ; any shortcut state is recorded or the legacy install is touched.
    ${If} $MnemarkRebrandAbort = 1
      DetailPrint "Update mode + legacy ClipFlow record detected; aborting cross-brand auto-update."
      MessageBox MB_YESNO|MB_ICONINFORMATION "Mnemark 全新更名版本已釋出。請前往以下網址重新下載並安裝： ${REBRAND_DOWNLOAD_URL}$\r$\n$\r$\nA newly renamed version of Mnemark has been released. Please download and install it again from: ${REBRAND_DOWNLOAD_URL}$\r$\n$\r$\n是否開啟下載頁面？ Open the download page?" IDYES open_rebrand_download
      Goto rebrand_abort
      open_rebrand_download:
        ExecShell "open" "${REBRAND_DOWNLOAD_URL}"
      rebrand_abort:
        Abort "Mnemark cannot auto-update over a legacy ClipFlow installation. Please download and reinstall Mnemark from the official release page."
    ${EndIf}

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
