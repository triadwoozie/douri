//! Packaging task for Dōri installers:
//! - Linux: dist/Dori-Setup-<version>-x86_64.AppImage
//! - Windows: dist/Dori-Setup-<version>-x86_64.exe

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

const VERSION: &str = "0.1.0-alpha.1";

fn run_cmd(mut cmd: Command, desc: &str) -> bool {
    println!("\n==> {desc} ({:?})", cmd);
    match cmd.status() {
        Ok(st) if st.success() => true,
        Ok(st) => {
            eprintln!("xtask package-installers error: `{desc}` failed with exit code: {st}");
            false
        }
        Err(e) => {
            eprintln!("xtask package-installers error: cannot run `{desc}`: {e}");
            false
        }
    }
}

fn add_dir_to_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    src_dir: &Path,
    prefix: &Path,
) -> std::io::Result<()> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    for entry in fs::read_dir(src_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let rel_path = prefix.join(&name);

        if path.is_dir() {
            zip.add_directory(rel_path.to_string_lossy(), options)?;
            add_dir_to_zip(zip, &path, &rel_path)?;
        } else {
            zip.start_file(rel_path.to_string_lossy(), options)?;
            let mut f = fs::File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}

fn create_zip(src_dir: &Path, zip_path: &Path) -> Result<(), String> {
    if let Some(p) = zip_path.parent() {
        let _ = fs::create_dir_all(p);
    }
    let file = fs::File::create(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    add_dir_to_zip(&mut zip, src_dir, Path::new("")).map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run(root: &Path) -> bool {
    let dist_dir = root.join("dist");
    let staging_dir = root.join("target").join("staging");
    let _ = fs::create_dir_all(&dist_dir);
    let _ = fs::create_dir_all(&staging_dir);

    println!("=======================================================");
    println!(" Packaging Dōri Installers v{VERSION}");
    println!(" Output Directory: {}", dist_dir.display());
    println!("=======================================================");

    // -------------------------------------------------------------
    // 1. Build Linux Release Binary
    // -------------------------------------------------------------
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root)
        .args(["build", "--release", "-p", "douri", "--bin", "douri"]);
    if !run_cmd(cmd, "Building Linux release binary (douri)") {
        return false;
    }

    let linux_bin = root.join("target").join("release").join("douri");
    if !linux_bin.is_file() {
        eprintln!("Missing compiled binary: {}", linux_bin.display());
        return false;
    }

    // -------------------------------------------------------------
    // 2. Package Linux Payload Zip
    // -------------------------------------------------------------
    let linux_payload_dir = staging_dir.join("linux_payload");
    let _ = fs::remove_dir_all(&linux_payload_dir);
    let _ = fs::create_dir_all(linux_payload_dir.join("bin"));
    let _ = fs::create_dir_all(linux_payload_dir.join("share").join("applications"));
    let _ = fs::create_dir_all(linux_payload_dir.join("share").join("icons"));
    let _ = fs::create_dir_all(
        linux_payload_dir
            .join("share")
            .join("mime")
            .join("packages"),
    );

    fs::copy(&linux_bin, linux_payload_dir.join("bin").join("douri")).expect("copy douri binary");
    fs::copy(
        root.join("dist")
            .join("linux")
            .join("io.github.triadwoozie.douri.desktop"),
        linux_payload_dir
            .join("share")
            .join("applications")
            .join("io.github.triadwoozie.douri.desktop"),
    )
    .expect("copy desktop file");
    fs::copy(
        root.join("dist")
            .join("linux")
            .join("io.github.triadwoozie.douri.png"),
        linux_payload_dir
            .join("share")
            .join("icons")
            .join("douri.png"),
    )
    .expect("copy icon");
    fs::copy(
        root.join("dist").join("linux").join("douri-mime.xml"),
        linux_payload_dir
            .join("share")
            .join("mime")
            .join("packages")
            .join("douri.xml"),
    )
    .expect("copy mime xml");

    let linux_payload_zip = staging_dir.join("linux-payload.zip");
    let _ = fs::remove_file(&linux_payload_zip);
    if let Err(e) = create_zip(&linux_payload_dir, &linux_payload_zip) {
        eprintln!("Failed to create linux-payload.zip: {e}");
        return false;
    }

    // -------------------------------------------------------------
    // 3. Build douri-installer for Linux Host with Embedded Payload
    // -------------------------------------------------------------
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root)
        .env("DOURI_PAYLOAD_ZIP", &linux_payload_zip)
        .args(["build", "--release", "-p", "douri-installer"]);
    if !run_cmd(cmd, "Building Linux douri-installer") {
        return false;
    }

    let linux_installer_bin = root.join("target").join("release").join("douri-installer");
    if !linux_installer_bin.is_file() {
        eprintln!(
            "Missing compiled installer: {}",
            linux_installer_bin.display()
        );
        return false;
    }

    // -------------------------------------------------------------
    // 4. Create Linux AppImage Installer
    // -------------------------------------------------------------
    let appdir = dist_dir.join("staging").join("installer.AppDir");
    let _ = fs::remove_dir_all(&appdir);
    let _ = fs::create_dir_all(appdir.join("usr").join("bin"));
    let _ = fs::create_dir_all(appdir.join("usr").join("share").join("applications"));
    let _ = fs::create_dir_all(
        appdir
            .join("usr")
            .join("share")
            .join("icons")
            .join("hicolor")
            .join("512x512")
            .join("apps"),
    );

    fs::copy(
        &linux_installer_bin,
        appdir.join("usr").join("bin").join("douri-installer"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(
            appdir.join("usr").join("bin").join("douri-installer"),
            fs::Permissions::from_mode(0o755),
        );
    }

    let desktop_content = "[Desktop Entry]\n\
Type=Application\n\
Name=Dōri Setup\n\
Comment=Install Dōri Digital Audio Workstation\n\
Exec=douri-installer\n\
Icon=io.github.triadwoozie.douri\n\
Terminal=false\n\
Categories=AudioVideo;Audio;AudioVideoEditing;\n";

    fs::write(
        appdir.join("io.github.triadwoozie.douri.desktop"),
        desktop_content,
    )
    .unwrap();
    fs::write(
        appdir
            .join("usr")
            .join("share")
            .join("applications")
            .join("io.github.triadwoozie.douri.desktop"),
        desktop_content,
    )
    .unwrap();
    fs::copy(
        root.join("dist")
            .join("linux")
            .join("io.github.triadwoozie.douri.png"),
        appdir.join("io.github.triadwoozie.douri.png"),
    )
    .unwrap();
    fs::copy(
        root.join("dist")
            .join("linux")
            .join("io.github.triadwoozie.douri.png"),
        appdir
            .join("usr")
            .join("share")
            .join("icons")
            .join("hicolor")
            .join("512x512")
            .join("apps")
            .join("io.github.triadwoozie.douri.png"),
    )
    .unwrap();

    let apprun_content = "#!/bin/sh\n\
SELF=$(readlink -f \"$0\")\n\
HERE=${SELF%/*}\n\
export PATH=\"${HERE}/usr/bin:${PATH}\"\n\
export XDG_DATA_DIRS=\"${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}\"\n\
exec \"${HERE}/usr/bin/douri-installer\" \"$@\"\n";

    fs::write(appdir.join("AppRun"), apprun_content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(appdir.join("AppRun"), fs::Permissions::from_mode(0o755));
        let _ =
            std::os::unix::fs::symlink("io.github.triadwoozie.douri.png", appdir.join(".DirIcon"));
    }

    let linux_appimage_out = dist_dir.join(format!("Dori-Setup-{VERSION}-x86_64.AppImage"));
    let _ = fs::remove_file(&linux_appimage_out);

    let mut appimagetool = Command::new("appimagetool");
    appimagetool
        .env("ARCH", "x86_64")
        .env("VERSION", VERSION)
        .env("APPIMAGE_EXTRACT_AND_RUN", "1");

    let runtime_file = root.join("dist").join("linux").join("runtime-x86_64");
    if runtime_file.is_file() {
        appimagetool.arg("--runtime-file").arg(&runtime_file);
    }

    appimagetool.arg(&appdir).arg(&linux_appimage_out);

    if !run_cmd(
        appimagetool,
        "Packaging Linux AppImage installer with appimagetool",
    ) {
        eprintln!("appimagetool packaging failed!");
        return false;
    }
    let _ = fs::remove_dir_all(&appdir);

    // -------------------------------------------------------------
    // 5. Build Windows Release Binary
    // -------------------------------------------------------------
    let mut cmd = Command::new("cargo-xwin");
    cmd.current_dir(root).args([
        "build",
        "--release",
        "--target",
        "x86_64-pc-windows-msvc",
        "-p",
        "douri",
        "--bin",
        "douri",
    ]);
    if !run_cmd(cmd, "Building Windows release binary (douri.exe)") {
        return false;
    }

    let win_bin = root
        .join("target")
        .join("x86_64-pc-windows-msvc")
        .join("release")
        .join("douri.exe");
    if !win_bin.is_file() {
        eprintln!("Missing Windows binary: {}", win_bin.display());
        return false;
    }

    // -------------------------------------------------------------
    // 6. Package Windows Payload Zip
    // -------------------------------------------------------------
    let win_payload_dir = staging_dir.join("windows_payload");
    let _ = fs::remove_dir_all(&win_payload_dir);
    let _ = fs::create_dir_all(&win_payload_dir);

    fs::copy(&win_bin, win_payload_dir.join("douri.exe")).expect("copy douri.exe");
    if root
        .join("dist")
        .join("windows")
        .join("README.txt")
        .is_file()
    {
        let _ = fs::copy(
            root.join("dist").join("windows").join("README.txt"),
            win_payload_dir.join("README.txt"),
        );
    }
    if root.join("LICENSE").is_file() {
        let _ = fs::copy(root.join("LICENSE"), win_payload_dir.join("LICENSE"));
    }

    let win_payload_zip = staging_dir.join("windows-payload.zip");
    let _ = fs::remove_file(&win_payload_zip);
    if let Err(e) = create_zip(&win_payload_dir, &win_payload_zip) {
        eprintln!("Failed to create windows-payload.zip: {e}");
        return false;
    }

    // -------------------------------------------------------------
    // 7. Build douri-installer for Windows (x86_64-pc-windows-msvc)
    // -------------------------------------------------------------
    let mut cmd = Command::new("cargo-xwin");
    cmd.current_dir(root)
        .env("DOURI_PAYLOAD_ZIP", &win_payload_zip)
        .args([
            "build",
            "--release",
            "--target",
            "x86_64-pc-windows-msvc",
            "-p",
            "douri-installer",
        ]);
    if !run_cmd(cmd, "Building Windows douri-installer.exe") {
        return false;
    }

    let win_installer_bin = root
        .join("target")
        .join("x86_64-pc-windows-msvc")
        .join("release")
        .join("douri-installer.exe");
    if !win_installer_bin.is_file() {
        eprintln!("Missing Windows installer: {}", win_installer_bin.display());
        return false;
    }

    let win_exe_out = dist_dir.join(format!("Dori-Setup-{VERSION}-x86_64.exe"));
    let _ = fs::remove_file(&win_exe_out);
    fs::copy(&win_installer_bin, &win_exe_out).expect("copy Dori-Setup.exe");

    // Clean up temporary payload staging directory
    let _ = fs::remove_dir_all(&staging_dir);

    // -------------------------------------------------------------
    // 8. Package Portables & RPM
    // -------------------------------------------------------------
    let win_portable_out = package_portable_windows(root, &dist_dir, &win_bin);
    let linux_portable_out = package_portable_linux(root, &dist_dir, &linux_bin);
    let rpm_ok = build_rpm(root);

    // -------------------------------------------------------------
    // 9. Assemble dist/release-<VERSION> and SHA256SUMS.txt
    // -------------------------------------------------------------
    assemble_release_artifacts(root, &dist_dir);

    println!("\n=======================================================");
    println!(" Release Artifacts Successfully Packaged!");
    println!("=======================================================");
    if let Ok(meta) = fs::metadata(&linux_appimage_out) {
        println!(
            "  -> Linux AppImage:   {} ({:.2} MB)",
            linux_appimage_out.display(),
            meta.len() as f64 / 1_048_576.0
        );
    }
    if let Ok(meta) = fs::metadata(&win_exe_out) {
        println!(
            "  -> Windows .exe:     {} ({:.2} MB)",
            win_exe_out.display(),
            meta.len() as f64 / 1_048_576.0
        );
    }
    println!("  -> Windows Portable: {win_portable_out}");
    println!("  -> Linux Portable:   {linux_portable_out}");
    println!("  -> Linux RPM:        {rpm_ok}");
    println!("=======================================================\n");

    true
}

pub fn build_rpm(root: &Path) -> bool {
    let script = root.join("dist").join("linux").join("build-rpm.sh");
    if !script.is_file() {
        eprintln!("Missing build-rpm.sh script at {}", script.display());
        return false;
    }
    let mut cmd = Command::new("bash");
    cmd.current_dir(root).arg(&script).arg(VERSION);
    run_cmd(cmd, "Building RPM package with dist/linux/build-rpm.sh")
}

fn package_portable_windows(root: &Path, dist_dir: &Path, win_bin: &Path) -> bool {
    let folder_name = format!("Dori-{VERSION}-win64-portable");
    let staging = dist_dir.join("staging").join(&folder_name);
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::create_dir_all(&staging);

    if fs::copy(win_bin, staging.join("douri.exe")).is_err() {
        eprintln!("Failed to copy douri.exe to portable staging");
        return false;
    }
    let readme = root.join("dist").join("windows").join("README.txt");
    if readme.is_file() {
        let _ = fs::copy(readme, staging.join("README.txt"));
    }
    let license = root.join("LICENSE");
    if license.is_file() {
        let _ = fs::copy(license, staging.join("LICENSE"));
    }

    let out_zip = dist_dir.join(format!("{folder_name}.zip"));
    let _ = fs::remove_file(&out_zip);
    let file = match fs::File::create(&out_zip) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create {}: {e}", out_zip.display());
            return false;
        }
    };
    let mut zip = zip::ZipWriter::new(file);
    if let Err(e) = add_dir_to_zip(&mut zip, &staging, Path::new(&folder_name)) {
        eprintln!("Failed to write portable zip: {e}");
        return false;
    }
    let _ = zip.finish();
    let _ = fs::remove_dir_all(dist_dir.join("staging"));
    true
}

fn package_portable_linux(root: &Path, dist_dir: &Path, linux_bin: &Path) -> bool {
    let folder_name = format!("Dori-{VERSION}-linux-x86_64-portable");
    let staging = dist_dir.join("staging").join(&folder_name);
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::create_dir_all(&staging);

    if fs::copy(linux_bin, staging.join("douri")).is_err() {
        eprintln!("Failed to copy douri to linux portable staging");
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(staging.join("douri"), fs::Permissions::from_mode(0o755));
    }
    let install_sh = root.join("dist").join("linux").join("install.sh");
    if install_sh.is_file() {
        let _ = fs::copy(&install_sh, staging.join("install.sh"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(
                staging.join("install.sh"),
                fs::Permissions::from_mode(0o755),
            );
        }
    }
    let desktop = root
        .join("dist")
        .join("linux")
        .join("io.github.triadwoozie.douri.desktop");
    if desktop.is_file() {
        let _ = fs::copy(
            &desktop,
            staging.join("io.github.triadwoozie.douri.desktop"),
        );
    }
    let icon = root
        .join("dist")
        .join("linux")
        .join("io.github.triadwoozie.douri.png");
    if icon.is_file() {
        let _ = fs::copy(&icon, staging.join("io.github.triadwoozie.douri.png"));
    }
    let mime = root.join("dist").join("linux").join("douri-mime.xml");
    if mime.is_file() {
        let _ = fs::copy(&mime, staging.join("douri-mime.xml"));
    }
    let readme = root.join("dist").join("README-linux.txt");
    if readme.is_file() {
        let _ = fs::copy(readme, staging.join("README.txt"));
    }
    let license = root.join("LICENSE");
    if license.is_file() {
        let _ = fs::copy(license, staging.join("LICENSE"));
    }

    let out_tar_gz = dist_dir.join(format!("{folder_name}.tar.gz"));
    let _ = fs::remove_file(&out_tar_gz);
    let mut cmd = Command::new("tar");
    cmd.current_dir(dist_dir.join("staging")).args([
        "-czf",
        out_tar_gz.to_str().unwrap(),
        &folder_name,
    ]);
    let ok = run_cmd(cmd, "Creating Linux portable tarball");
    let _ = fs::remove_dir_all(dist_dir.join("staging"));
    ok
}

fn assemble_release_artifacts(_root: &Path, dist_dir: &Path) {
    let release_dir = dist_dir.join(format!("release-{VERSION}"));
    let _ = fs::remove_dir_all(&release_dir);
    let _ = fs::create_dir_all(&release_dir);

    let artifacts = [
        format!("Dori-Setup-{VERSION}-x86_64.exe"),
        format!("Dori-{VERSION}-win64-portable.zip"),
        format!("Dori-Setup-{VERSION}-x86_64.AppImage"),
        format!("Dori-{VERSION}-linux-x86_64-portable.tar.gz"),
        format!("dori-{VERSION}-1.x86_64.rpm"),
    ];

    for name in &artifacts {
        let src = dist_dir.join(name);
        let dst = release_dir.join(name);
        if src.is_file() {
            let _ = fs::copy(&src, &dst);
        } else {
            eprintln!("Warning: artifact missing: {}", src.display());
        }
    }

    // Generate SHA256SUMS.txt
    let mut cmd = Command::new("sh");
    cmd.current_dir(&release_dir)
        .arg("-c")
        .arg("sha256sum Dori* dori* > SHA256SUMS.txt 2>/dev/null || true");
    let _ = cmd.status();
}
