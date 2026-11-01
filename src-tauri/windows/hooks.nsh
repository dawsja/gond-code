; Gond Code installer hooks.
;
; Builds before 0.1.0 put a copy of the app in <install>\bin and that folder on the user's PATH.
; Installing this version takes both away again; `gond-code path remove` edits PATH in Rust with
; the registry API (NSIS strings stop at NSIS_MAX_STRLEN and would cut, and so wipe, a long
; PATH), keeping the value's REG_EXPAND_SZ type and broadcasting WM_SETTINGCHANGE.

!macro GOND_REMOVE_OLD_BIN
  ${If} ${FileExists} "$INSTDIR\bin\*.*"
    nsExec::ExecToLog '"$INSTDIR\${MAINBINARYNAME}.exe" path remove "$INSTDIR\bin"'
    Pop $0
    Delete "$INSTDIR\bin\gond-code.exe"
    RMDir "$INSTDIR\bin"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro GOND_REMOVE_OLD_BIN
!macroend

; "Delete the application data": Tauri's checkbox only removes the bundle-id folders (WebView2
; data). Gond Code's own data lives in %APPDATA%\GondCode, %LOCALAPPDATA%\GondCode and Credential
; Manager (GondCode/...), so `gond-code purge-data` removes those, and nothing else. It runs after
; the app is closed (post-uninstall), from a copy taken while the exe still exists, and never on
; an update (the updater's /UPDATE uninstall). `/PURGE` on the command line does the same for
; scripted uninstalls (the box can't be ticked under /S).
Var GondPurge

; The confirm page says plainly what ticking the box costs.
!define MUI_UNCONFIRMPAGE_TEXT_TOP "Gond Code will be removed from this PC. If you tick $\"Delete the application data$\", your sessions, settings and saved accounts are deleted too. Your project folders and the agents' own settings are never touched."

!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $GondPurge 0
  ${If} $DeleteAppDataCheckboxState = 1
    StrCpy $GondPurge 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $CMDLINE "/PURGE" $R9
  ${IfNot} ${Errors}
    StrCpy $GondPurge 1
  ${EndIf}
  ${If} $UpdateMode = 1
    StrCpy $GondPurge 0
  ${EndIf}
  ${If} $GondPurge = 1
    CopyFiles /SILENT "$INSTDIR\${MAINBINARYNAME}.exe" "$TEMP\gond-code-purge.exe"
  ${EndIf}
  ; The speech-to-text model (731 MB, Settings → General) goes on every uninstall, data
  ; kept or not; never on an update.
  ${If} $UpdateMode <> 1
    RMDir /r "$LOCALAPPDATA\GondCode\speech"
  ${EndIf}
  ; The terminal host keeps terminals running after the app closes, and through an update.
  ; Uninstalling closes them and removes the host's copy of the exe; an update leaves it be.
  ${If} $UpdateMode <> 1
    nsExec::ExecToLog '"$INSTDIR\${MAINBINARYNAME}.exe" pty-host stop'
    Pop $0
    Sleep 500
    RMDir /r "$LOCALAPPDATA\GondCode\host"
  ${EndIf}
  !insertmacro GOND_REMOVE_OLD_BIN
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $GondPurge = 1
    nsExec::ExecToLog '"$TEMP\gond-code-purge.exe" purge-data'
    Pop $0
    StrCmp $0 "0" +2
      DetailPrint "Some of Gond Code's data couldn't be deleted ($0)."
    Delete "$TEMP\gond-code-purge.exe"
  ${EndIf}
!macroend
