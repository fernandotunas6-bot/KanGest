@echo off
rem KanGest - exporta os dados do KIGEST (SQL Server) para um ficheiro .kigest
rem Corre no computador onde esta a base do KIGEST. So le a base: nao altera nada.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0ExportarKIGEST.ps1" %*
