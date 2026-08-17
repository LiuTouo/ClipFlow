; Deterministic proof for the Mnemark rebrand auto-update guard.
;
; Compile with makensis, then run guard-proof.exe. It drives the REAL
; REBRAND_GUARD_DECIDE macro (shared with installer-hooks.nsh) across the
; three required scenarios and writes a PASS/FAIL line per scenario to
; guard-proof-results.txt next to the exe. A non-zero exit code means a
; scenario failed.
;
;   A  UpdateMode=1 + legacy=1  -> MnemarkRebrandAbort must be 1 (notice+abort)
;   B  UpdateMode=0 + legacy=1  -> must be 0 (manual migration continues)
;   C  UpdateMode=1 + legacy=0  -> must be 0 (normal Mnemark update continues)

!include LogicLib.nsh
!include "installer-hooks.nsh"

Var UpdateMode
Var FailCount

Name "Mnemark rebrand guard proof"
OutFile "guard-proof.exe"
RequestExecutionLevel user
SilentInstall silent
ShowInstDetails nevershow

Section
  StrCpy $FailCount 0
  FileOpen $0 "$EXEDIR\guard-proof-results.txt" w

  ; A: auto-update + legacy -> guard must fire
  StrCpy $UpdateMode 1
  StrCpy $MnemarkLegacyFound 1
  !insertmacro REBRAND_GUARD_DECIDE
  ${If} $MnemarkRebrandAbort = 1
    FileWrite $0 "A:PASS update+legacy aborts$\r$\n"
  ${Else}
    FileWrite $0 "A:FAIL update+legacy did not abort$\r$\n"
    IntOp $FailCount $FailCount + 1
  ${EndIf}

  ; B: manual install + legacy -> migration path must continue
  StrCpy $UpdateMode 0
  StrCpy $MnemarkLegacyFound 1
  !insertmacro REBRAND_GUARD_DECIDE
  ${If} $MnemarkRebrandAbort = 0
    FileWrite $0 "B:PASS manual+legacy continues$\r$\n"
  ${Else}
    FileWrite $0 "B:FAIL manual+legacy wrongly aborts$\r$\n"
    IntOp $FailCount $FailCount + 1
  ${EndIf}

  ; C: auto-update + no legacy -> normal update must continue
  StrCpy $UpdateMode 1
  StrCpy $MnemarkLegacyFound 0
  !insertmacro REBRAND_GUARD_DECIDE
  ${If} $MnemarkRebrandAbort = 0
    FileWrite $0 "C:PASS update+no-legacy continues$\r$\n"
  ${Else}
    FileWrite $0 "C:FAIL update+no-legacy wrongly aborts$\r$\n"
    IntOp $FailCount $FailCount + 1
  ${EndIf}

  FileClose $0
  SetErrorLevel $FailCount
SectionEnd
