!include "Win\RestartManager.nsh"

; Never request shutdown: the app may be changing files or saving a session.
; An unavailable process check must not authorize replacement/removal.
!macro ProjectPaneCheckClosed executablePath
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  Push $R0
  Push $R1
  StrCpy $R1 0
  System::Call 'kernel32::GetTickCount() i .r5'
  ${Do}
    ; A fresh RM session avoids retaining a stopped process from an earlier list.
    !insertmacro RestartManager_StartSession $R0
    ${If} $R0 == ""
      SetErrorLevel 2
      Abort "ProjectPane: unable to check running processes. Close the app and retry."
    ${EndIf}
    !insertmacro RestartManager_RegisterFile $R0 "${executablePath}"
    ${If} $0 != 0
      !insertmacro RestartManager_EndSession $R0
      SetErrorLevel 2
      Abort "ProjectPane: unable to check the target file. Close the app and retry."
    ${EndIf}
    StrCpy $1 0
    StrCpy $2 0
    StrCpy $3 0
    System::Call 'RSTRTMGR::RmGetList(i $R0, *i .r1, *i r2, p 0, *i .r3) i .r4'
    !insertmacro RestartManager_EndSession $R0
    System::Call 'kernel32::GetTickCount() i .r2'
    IntOp $2 $2 - $5
    ; The updater starts this installer immediately before exiting its host.
    ; Wait only for an identified busy target, never for a failed access check.
    ${If} $UpdateMode != 1
    ${OrIf} $R1 >= 100
    ${OrIf} $2 >= 10000
    ${OrIf} $2 < 0
      ${ExitDo}
    ${EndIf}
    ${If} $4 != 234
      ${ExitDo}
    ${EndIf}
    Sleep 100
    IntOp $R1 $R1 + 1
  ${Loop}
  ${If} $4 != 0
  ${OrIf} $1 != 0
    SetErrorLevel 2
    Abort "ProjectPane is running or its state cannot be checked. Close it normally, wait for file operations to finish, then retry."
  ${EndIf}
  Pop $R1
  Pop $R0
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
!macroend
