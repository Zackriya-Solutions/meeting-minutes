@echo off
setlocal

rem Always run relative to this script, even when invoked from the repository root.
pushd "%~dp0" || exit /b 1

rem The repository is a Cargo workspace, whose default target directory is at the
rem repository root. Override it so Tauri puts every Windows artifact where the
rem frontend build documentation says it will be.
set "CARGO_TARGET_DIR=%CD%\src-tauri\target"
set "RELEASE_DIR=%CARGO_TARGET_DIR%\release"

echo Cleaning npm dependencies...
if exist "node_modules" rd /s /q "node_modules"
if errorlevel 1 goto :clean_failed
if exist "package-lock.json" del /f /q "package-lock.json"
if errorlevel 1 goto :clean_failed

echo Installing npm dependencies...
rem pnpm is pnpm.cmd on Windows. CALL is required or this batch file ends here.
call pnpm install
if errorlevel 1 goto :install_failed
call pnpm run version:check
if errorlevel 1 goto :version_check_failed

echo Building llama-helper sidecar...
call cargo build --release -p llama-helper
if errorlevel 1 goto :helper_build_failed

echo Copying llama-helper sidecar to binaries...
if not exist "src-tauri\binaries" mkdir "src-tauri\binaries"
copy /Y "%RELEASE_DIR%\llama-helper.exe" "src-tauri\binaries\llama-helper-x86_64-pc-windows-msvc.exe" >nul
if errorlevel 1 goto :helper_copy_failed

echo Building the project...
call pnpm run tauri:build
if errorlevel 1 goto :tauri_build_failed

rem Cargo names the binary after the Rust package (meet4specs), while productName is
rem Meet4Specs. On Windows this is only a casing difference, so rename through
rem a temporary filename to expose the documented product-name path.
if exist "%RELEASE_DIR%\meet4specs.exe" (
    ren "%RELEASE_DIR%\meet4specs.exe" "Meet4Specs.tmp.exe"
    if errorlevel 1 goto :artifact_validation_failed
    ren "%RELEASE_DIR%\Meet4Specs.tmp.exe" "Meet4Specs.exe"
    if errorlevel 1 goto :artifact_validation_failed
)

rem Do not report success unless every documented artifact was really produced.
if not exist "out\index.html" goto :artifact_validation_failed
if not exist "%RELEASE_DIR%\Meet4Specs.exe" goto :artifact_validation_failed
dir /b "%RELEASE_DIR%\bundle\nsis\*_x64-setup.exe" >nul 2>&1
if errorlevel 1 goto :artifact_validation_failed
dir /b "%RELEASE_DIR%\bundle\msi\*_x64_en-US.msi" >nul 2>&1
if errorlevel 1 goto :artifact_validation_failed

echo.
echo Build completed successfully. Artifacts:
echo   Static frontend: %CD%\out
echo   Raw executable:  %RELEASE_DIR%\Meet4Specs.exe
echo   NSIS installer:  %RELEASE_DIR%\bundle\nsis
echo   MSI installer:   %RELEASE_DIR%\bundle\msi
popd
exit /b 0

:clean_failed
echo ERROR: Failed to clean npm dependencies.
goto :failed
:install_failed
echo ERROR: Failed to install npm dependencies.
goto :failed
:version_check_failed
echo ERROR: Application versions are not synchronized.
goto :failed
:helper_build_failed
echo ERROR: Failed to build llama-helper.
goto :failed
:helper_copy_failed
echo ERROR: Failed to copy llama-helper binary.
goto :failed
:tauri_build_failed
echo ERROR: Failed to build the Tauri application.
goto :failed
:artifact_validation_failed
echo ERROR: Build finished without all expected artifacts.
goto :failed
:failed
popd
exit /b 1
