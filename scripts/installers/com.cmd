@echo off
rem `com` launcher of the standalone install (scripts/build-installers.ts): runs the CLI on the Bun and
rem the engine shipped next to it, so the machine needs neither Node, npm nor Bun.
setlocal
set "COM_DIR=%~dp0"
if not defined COM_BUN set "COM_BUN=%COM_DIR%runtime\bun.exe"
if not defined COM_COMPILER_PATH set "COM_COMPILER_PATH=%COM_DIR%runtime\com-compiler.exe"
"%COM_DIR%runtime\bun.exe" "%COM_DIR%lib\bin\cli.ts" %*
exit /b %ERRORLEVEL%
