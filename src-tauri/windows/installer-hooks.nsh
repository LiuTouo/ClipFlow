; Force-close any running ClipFlow before the installer overwrites clipflow.exe.
; The stock CheckIfAppIsRunning can miss the process (TOCTOU on the snapshot)
; and leave the exe locked, failing the File write. taskkill /F /T is a
; forceful, idempotent kill; its "not found" exit code is ignored.
; ponytail: /IM matches every user's clipflow.exe; per-user filter (taskkill /FI
; "USERNAME eq ...") only if multi-user RDP on one box ever matters.

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Closing running ClipFlow..."
  nsExec::ExecToStack 'taskkill.exe /F /T /IM ${MAINBINARYNAME}.exe'
  Pop $0 ; exit code (ignored — non-zero just means nothing was running)
  Pop $1 ; stdout
  Sleep 500
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Closing running ClipFlow..."
  nsExec::ExecToStack 'taskkill.exe /F /T /IM ${MAINBINARYNAME}.exe'
  Pop $0
  Pop $1
  Sleep 500
!macroend
