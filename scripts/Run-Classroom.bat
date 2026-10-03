@echo off
setlocal EnableExtensions DisableDelayedExpansion
title Classroom - Prepare local test

rem Run this file in Windows. The executable is supplied by the Mac shared folder.
set "CLASSROOM_SOURCE=\\Mac\Home\Downloads\Classroom.exe"
set "CLASSROOM_LOG_DIR=\\Mac\Home\Downloads\logs"
if not defined LOCALAPPDATA goto no_local_appdata
set "CLASSROOM_RUN_DIR=%LOCALAPPDATA%\Classroom\Loopback"

if not exist "%CLASSROOM_SOURCE%" goto no_source
if not exist "%CLASSROOM_RUN_DIR%\" mkdir "%CLASSROOM_RUN_DIR%"
if not exist "%CLASSROOM_RUN_DIR%\" goto copy_failed

rem Stage the copy so a failed transfer does not replace the previous executable.
copy /b /y "%CLASSROOM_SOURCE%" "%CLASSROOM_RUN_DIR%\Classroom.new.exe" >nul
if errorlevel 1 goto copy_failed
move /y "%CLASSROOM_RUN_DIR%\Classroom.new.exe" "%CLASSROOM_RUN_DIR%\Classroom.exe" >nul
if errorlevel 1 goto copy_failed

rem Remove a download-zone marker from this executable only, if one is present.
powershell.exe -NoLogo -NoProfile -NonInteractive -Command "Unblock-File -LiteralPath (Join-Path $env:CLASSROOM_RUN_DIR 'Classroom.exe') -ErrorAction Stop"
if errorlevel 1 goto unblock_failed

cd /d "%CLASSROOM_RUN_DIR%"
if errorlevel 1 goto copy_failed

rem Student retries automatically while Teacher starts capture and its encoder.
rem /D disables CMD AutoRun; /K keeps each console open if the application exits.
start "Classroom - Teacher" "%ComSpec%" /D /K "Classroom.exe --role teacher --loopback --log-file %CLASSROOM_LOG_DIR%\teacher.log"
if errorlevel 1 goto launch_failed
start "Classroom - Student" "%ComSpec%" /D /K "Classroom.exe --role student --loopback --log-file %CLASSROOM_LOG_DIR%\student.log"
if errorlevel 1 goto launch_failed
exit /b 0

:no_source
echo ERROR: Cannot find the source executable:
echo "%CLASSROOM_SOURCE%"
echo Make sure Mac folder sharing is available and Classroom.exe is in Mac Downloads.
goto failed

:no_local_appdata
echo ERROR: Windows LOCALAPPDATA is unavailable. Run this file inside Windows.
goto failed

:copy_failed
echo ERROR: Could not copy Classroom.exe into:
echo "%CLASSROOM_RUN_DIR%"
echo Close any previous Classroom Teacher and Student windows, then try again.
goto failed

:unblock_failed
echo ERROR: Could not prepare the local executable for launch.
echo See the PowerShell error above.
goto failed

:launch_failed
echo ERROR: Could not open a Classroom console.
goto failed

:failed
echo.
pause
exit /b 1
