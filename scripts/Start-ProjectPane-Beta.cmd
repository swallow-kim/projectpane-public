@echo off
setlocal
if not exist "%~dp0projectpane.exe" goto missing
if not exist "%~dp0projectpane-read-worker.exe" goto missing
set "PROJECTPANE_DATA_DIR=%~dp0data"
set "WEBVIEW2_USER_DATA_FOLDER=%~dp0webview-profile"
set "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="
if not exist "%~dp0data\foundation\projects-v1\settings.json" goto first
start "" /D "%~dp0" "%~dp0projectpane.exe"
exit /b 0
:first
start "" /D "%~dp0" "%~dp0projectpane.exe" --folder "%~dp0."
exit /b 0
:missing
echo Extract the complete ProjectPane beta ZIP before running this launcher.
pause
exit /b 1
