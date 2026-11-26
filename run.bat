@echo off
cd /d "%~dp0"
where pythonw >nul 2>&1
if errorlevel 1 (
  echo pythonw پیدا نشد. Python را نصب کنید یا مسیر آن را به PATH اضافه کنید.
  pause
  exit /b 1
)
start "" pythonw "app.py"