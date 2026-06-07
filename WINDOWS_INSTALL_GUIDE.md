# Comprehensive Windows 10/11 Installation Guide for Meetily

This guide provides a complete, step-by-step walkthrough for installing the Meetily (meeting-minutes) application on a Windows system. It is based on a real-world installation and is designed to help you avoid common errors related to missing dependencies and script failures.

---

### **Phase 1: Install All Prerequisites**

Before you download or run any project files, you must install three essential development tools. **Do not skip this phase.**

#### **1. Install Git for Windows**
Git is required to download the project files from GitHub.

1.  **Download:** Go to [git-scm.com/download/win](https://git-scm.com/download/win) and download the installer.
2.  **Install:** Run the installer. During installation, accept the default settings for most steps. On the **"Adjusting your PATH environment"** screen, ensure you select **"Git from the command line and also from 3rd-party software"**.
3.  **Verify:** After installation, open a new Command Prompt and type `git --version`. If it shows a version number, you have succeeded.

#### **2. Install CMake**
CMake is a tool that prepares the C++ source code for compilation.

1.  **Download:** Go to [cmake.org/download](https://cmake.org/download/). Under "Binary distributions," download the "Windows x64 Installer" (the file ending in `.msi`).
2.  **Install:** Run the `.msi` installer.
3.  **Critical Step:** During installation, you will reach an "Install Options" screen. You **must** select **"Add CMake to the system PATH for all users"** or **"Add CMake to the system PATH for the current user."** This step is essential.
4.  **Verify:** After installation, open a new Command Prompt and type `cmake --version`. If it shows a version number, you have succeeded.

#### **3. Install Visual Studio Build Tools**
This package contains the actual C++ compiler (`nmake.exe`) needed to build the backend.

1.  **Download:** Go to [visualstudio.microsoft.com/downloads](https://visualstudio.microsoft.com/downloads/). Scroll down to **"Tools for Visual Studio"** and download the **"Build Tools for Visual Studio"**.
2.  **Install:** Run the downloaded `vs_buildtools.exe`. This will launch the Visual Studio Installer.
3.  **Select Workload:** In the "Workloads" tab, you **must** check the box for **"Desktop development with C++"**. This is the only component needed.
4.  **Install:** Click "Install" and wait for the process to complete. This will take some time.

---

### **Phase 2: Backend Setup & Build**

With all prerequisites installed, you can now build the application backend.

1.  **Clone the Repository:**
    Open a standard Command Prompt (or terminal of your choice) and run the following command to download the project:
    ```cmd
    git clone [https://github.com/Zackriya-Solutions/meeting-minutes](https://github.com/Zackriya-Solutions/meeting-minutes)
    ```

2.  **Open the Correct Terminal:**
    * Click your Start Menu and type `Developer Command Prompt`.
    * Click to open the **"Developer Command Prompt for VS"**. You will perform the rest of the backend setup in this specific terminal window.

3.  **Navigate to the Backend Directory:**
    In the Developer Command Prompt, use the `cd` command to navigate into the project's backend folder.
    ```cmd
    cd C:\path\to\your\meeting-minutes\backend
    ```
    *(Replace `C:\path\to\your` with the actual path, e.g., `C:\Users\natha`)*

4.  **Run the Build Script:**
    Now, run the build script. Because all dependencies are installed, it should complete successfully.
    ```cmd
    .\build_whisper.cmd
    ```

5.  **Select Your AI Model:**
    The script will pause and ask you to choose a transcription model.
    * **Recommendation for good balance:** `medium.en`
    * **Recommendation for most hardware (CPU-optimized):** `medium.en-q8_0`
    Type your chosen model name and press Enter. The script will download the model and finish the build.

---

### **Phase 3: Frontend Installation**

This is the user interface you will interact with.

1.  **Download:** Download the recommended setup executable:
    * [meetily-frontend_0.0.4_x64-setup.exe](https://github.com/Zackriya-Solutions/meeting-minutes/releases/download/v0.0.4-alpha/meetily-frontend_0.0.4_x64-setup.exe)
2.  **Install:** Run the installer and follow the on-screen instructions, just like any other Windows application.

---

### **Phase 4: Running the Application**

To use the application, you must first start the backend server and then launch the frontend GUI.

**Option A: The Manual Way**

1.  **Start Backend:** Open the **"Developer Command Prompt for VS"**, navigate to the backend directory (`cd C:\path\to\your\meeting-minutes\backend`), and run:
    ```cmd
    powershell.exe -ExecutionPolicy Bypass -File .\start_with_output.ps1
    ```
    **Keep this window open.**
2.  **Start Frontend:** Launch the "Meetily" application from your Start Menu or desktop shortcut.

**Option B: One-Click Launch Script (Recommended)**

This method uses a helper tool to launch the backend silently on a separate virtual desktop.

1.  **Download Helper Tool:**
    * Go to [https://github.com/dankrusi/vd/releases](https://github.com/dankrusi/vd/releases).
    * Download the `vd.exe` file.
    * Place the downloaded `vd.exe` file directly inside your backend folder (`C:\Users\natha\meeting-minutes\backend\`).

2.  **Create the Script:**
    * Open Notepad.
    * Copy the code block below and paste it into Notepad.
    * **Verify the paths** in the script match your system.
    * Save the file to your Desktop as `Launch Meetily.bat` (ensure "Save as type" is set to "All Files").

    ```batch
    @echo off
    REM =================================================================
    REM ==        Meetily - Custom Launch Script (v4)                ==
    REM ==        FIX: Launches backend on a new virtual desktop.    ==
    REM =================================================================
    echo.
    echo =================================================================
    echo      Initializing the Developer Environment...
    echo =================================================================
    echo.

    REM --- This is the path to the Visual Studio Developer Command Prompt tools.
    SET VS_DEV_CMD="C:\Program Files\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"

    REM --- This is the path to your meeting-minutes backend directory.
    SET BACKEND_DIR="C:\Users\natha\meeting-minutes\backend"

    REM --- This is the path to where the Meetily frontend was installed.
    SET FRONTEND_EXE="C:\Program Files\Meetily\meetily-frontend.exe"

    REM --- Initialize the developer command prompt environment.
    call %VS_DEV_CMD%

    REM --- Navigate to the backend directory.
    cd /d %BACKEND_DIR%

    echo.
    echo =================================================================
    echo      Creating New Desktop & Starting Backend...
    echo =================================================================
    echo.

    REM --- Use vd.exe to run the backend on a new virtual desktop.
    REM --- 'runnew' creates a new desktop, runs the command, then switches back.
    vd.exe runnew powershell.exe -NoExit -Command ".\start_with_output.ps1"

    echo.
    echo =================================================================
    echo      Launching the Frontend GUI...
    echo =================================================================
    echo.

    REM --- A short delay to give the backend a moment to initialize.
    timeout /t 5 /nobreak > nul

    REM --- This launches the frontend application on the current desktop.
    start "" %FRONTEND_EXE%

    exit
    ```

3.  **Launch:** Double-click the `Launch Meetily.bat` script on your desktop to start the backend silently on another desktop and launch the frontend on your main screen.
