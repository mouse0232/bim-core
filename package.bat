@echo off
setlocal

echo Bim-Core 打包脚本
echo =================
echo.

REM 设置版本号
set VERSION=1.0.0

REM 创建发布目录
echo 创建发布目录...
if exist "release" rmdir /s /q "release"
mkdir "release"

REM 复制可执行文件
echo 复制可执行文件...
copy "target\release\bim.exe" "release\"
copy "target\release\bimc.exe" "release\"

REM 复制文档文件
echo 复制文档文件...
if exist "README.md" copy "README.md" "release\" >nul 2>&1
if exist "DEVELOPMENT.md" copy "DEVELOPMENT.md" "release\" >nul 2>&1
if exist "DEBUGGING.md" copy "DEBUGGING.md" "release\" >nul 2>&1
if exist "LICENSE" copy "LICENSE" "release\" >nul 2>&1

REM 创建测试脚本
echo 创建测试脚本...
(
echo @echo off
echo echo Bim-Core 测试
echo echo =============
echo echo.
echo echo 1. 启动服务器 (在另一个终端运行):
echo echo    bim.exe 127.0.0.1:8080
echo echo.
echo echo 2. 运行客户端测试:
echo echo    bimc.exe http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo echo.
echo echo 3. 查看帮助信息:
echo echo    bim.exe --help
echo echo    bimc.exe --help
echo pause
) > release\test.bat

REM 创建IPv4/IPv6测试脚本
echo 创建IPv4/IPv6测试脚本...
(
echo @echo off
echo echo Bim-Core IPv4/IPv6 测试
echo echo ========================
echo echo.
echo echo 1. 默认模式（自动选择IP版本^):
echo bimc.exe http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo echo.
echo echo 2. 强制使用IPv4 (-4^):
echo bimc.exe -4 http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo echo.
echo echo 3. 强制使用IPv6 (-6^):
echo bimc.exe -6 http://[::1]:8080/download http://[::1]:8080/upload
echo echo.
echo echo 4. 多线程 + IPv4:
echo bimc.exe -4 -m 4 http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo echo.
echo echo 5. 多线程 + IPv6:
echo bimc.exe -6 -m 4 http://[::1]:8080/download http://[::1]:8080/upload
echo echo.
echo pause
) > release\ipv4_ipv6_test.bat

REM 创建版本信息文件
echo 创建版本信息文件...
(
echo Bim-Core v%VERSION%
echo.
echo 编译时间: %date% %time%
echo.
echo 包含文件:
echo - bim.exe     - 服务器程序
echo - bimc.exe    - 客户端程序
echo - README.md   - 使用说明
echo - LICENSE     - 许可证
) > release\VERSION.txt

REM 打包为zip文件
echo 创建zip压缩包...
if exist "bim-core-v%VERSION%.zip" del "bim-core-v%VERSION%.zip"
powershell -command "Compress-Archive -Path release\* -DestinationPath bim-core-v%VERSION%.zip"

echo.
echo 打包完成！
echo ---------
echo 可执行文件: 
echo   - target\release\bim.exe  (服务器程序)
echo   - target\release\bimc.exe (客户端程序)
echo.
echo 发布包:
echo   - release\ 目录包含所有发布文件
echo   - bim-core-v%VERSION%.zip 压缩包
echo.
pause