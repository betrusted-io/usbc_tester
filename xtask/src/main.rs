use std::{
    env,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use const_format::formatcp;

type DynError = Box<dyn std::error::Error>;

const TARGET: &str = "riscv32imc-unknown-none-elf";
const IMAGE_PATH: &'static str = formatcp!("target/{}/release/usbc_img.bin", TARGET);
const DEST_FILE: &'static str = formatcp!("usbc_img.bin");
const DESTDIR: &'static str = "code/precursors/";

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{}", e);
        std::process::exit(-1);
    }
}

fn try_main() -> Result<(), DynError> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("hw-image") => build_hw_image(false, env::args().nth(2))?,
        Some("docs") => make_docs()?,
        Some("push") => push_to_pi(env::args().nth(2), env::args().nth(3))?,
        Some("stage-fw") => update_usb(true)?,
        Some("copy-precursors") => copy_precursors()?,
        _ => print_help(),
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "Tasks:
hw-image [soc.svd]      builds an image for real hardware
docs                    updates the documentation tree
push  [ip] [id]         deploys files to burner Rpi. Example: push 192.168.1.2 ~/id_rsa. Assumes 'pi' as the user.
stage-fw                stages the EC firmware and gateware for burn
copy-precursors         copy precursors from a local build of the FPGA to the default location used by xtask
"
    )
}

fn scp(addr: &str, username: &str, idfile: Option<String>, src_file: &std::path::Path, dest_file: &std::path::Path) {
    use std::io::prelude::*;

    let tcp = std::net::TcpStream::connect(addr).unwrap();
    let mut sess = ssh2::Session::new().unwrap();

    sess.set_timeout(10000);
    sess.set_tcp_stream(tcp);
    sess.handshake().unwrap();

    if idfile.is_some() {
        sess.userauth_pubkey_file(username, None, &PathBuf::from(idfile.unwrap()), None).unwrap();
    } else {
        sess.userauth_agent(username).unwrap();
    }

    let mut f = std::fs::File::open(src_file).unwrap();
    let mut f_data = vec![];
    f.read_to_end(&mut f_data).unwrap();

    println!("Copying {:?} to {:?} on host {:?}", src_file, dest_file, addr);
    let mut remote_file = sess
        .scp_send(dest_file.as_ref(), 0o644, f_data.len() as _, None)
        .unwrap();
    remote_file.write_all(&f_data).unwrap();
}

fn push_to_pi(target: Option<String>, id: Option<String>) -> Result<(), DynError> {

    let target_str = match target {
        Some(tgt) => tgt + ":22",
        _ => {println!("Must specify a target for push."); return Err("Must specify a target for push".into())},
    };

    // print some short, non-cryptographic checksums so we can easily sanity check versions across machines
    let mut csr_vec = Vec::new();
    let mut csr_file = std::fs::File::open("precursors/csr.csv")?;
    csr_file.read_to_end(&mut csr_vec)?;
    let digest = md5::compute(&csr_vec);
    print!("csr.csv: {}\n", format!("{:x}", digest));

    let mut image_vec = Vec::new();
    let mut image_file = std::fs::File::open(IMAGE_PATH)?;
    image_file.read_to_end(&mut image_vec)?;
    let digest_image = md5::compute(&image_vec);
    print!("bt-ec.bin: {}\n", format!("{:x}", digest_image));

    let dest_str = DESTDIR.to_string() + DEST_FILE;
    let dest = Path::new(&dest_str);
    scp(&target_str.clone(), "pi", id.clone(), Path::new(&IMAGE_PATH), &dest);

    let dest_str = DESTDIR.to_string() + "usbc_tester_csr.csv";
    let dest = Path::new(&dest_str);
    scp(&target_str.clone(), "pi", id.clone(), Path::new("precursors/csr.csv"), &dest);

    Ok(())
}

fn update_usb(do_ec: bool) -> Result<(), DynError> {
    use std::process::Stdio;
    use std::io::{BufRead, BufReader, Error, ErrorKind};

    if do_ec {
        println!("Staging EC objects");
        let stdout = Command::new("python3")
        .arg("tools/usb_update.py")
        .arg("-e")
        .arg("precursors/usbc_tester.bin")
        .stdout(Stdio::piped())
        .spawn()?
        .stdout
        .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture output"))?;

        let reader = BufReader::new(stdout);
        reader.lines().for_each(|line|
            println!("{}", line.unwrap())
        );
    }

    Ok(())
}

fn copy_precursors() -> Result<(), DynError> {
    println!("copying csr.csv, soc.svd, and usbc_tester.bin from default build location to precursors/...");
    std::fs::copy("build/csr.csv", "precursors/csr.csv")?;
    std::fs::copy("build/software/soc.svd", "precursors/soc.svd")?;
    std::fs::copy("build/gateware/usbc_tester.bin", "precursors/usbc_tester.bin")?;
    Ok(())
}

fn make_docs() -> Result<(), DynError> {
    Command::new("sphinx-build")
    .arg("-M").arg("html")
    .arg("build/documentation")
    .arg("build/documentation/_build")
    .output()
    .expect("Failed to build docs");

    Ok(())
}

fn build_hw_image(debug: bool, svd: Option<String>) -> Result<(), DynError> {
    let svd_file = match svd {
        Some(s) => s,
        None => {println!("Using default soc.svd location of precursors/soc.svd"); "precursors/soc.svd".to_string() },
    };

    let path = std::path::Path::new(&svd_file);
    if !path.exists() {
        return Err("svd file does not exist".into());
    }

    // Tools use this environment variable to know when to rebuild the UTRA crate.
    std::env::set_var("EC_SVD_FILE", path.canonicalize().unwrap());

    let sw = build_sw(debug)?;

    let loaderpath = PathBuf::from("sw/loader.S");
    let gatewarepath = PathBuf::from("precursors/usbc_tester.bin");
    let output_bundle = create_image(&sw, &loaderpath, &gatewarepath)?;
    println!();
    println!(
        "USBC tester software image bundle is available at {}",
        output_bundle.display()
    );

    Ok(())
}


fn build_sw(debug: bool) -> Result<PathBuf, DynError> {
    build("sw", debug, Some(TARGET), Some("sw".into()))
}

fn build(
    project: &str,
    debug: bool,
    target: Option<&str>,
    directory: Option<PathBuf>,
) -> Result<PathBuf, DynError> {
    println!("Building {}...", project);
    let stream = if debug { "debug" } else { "release" };
    let mut args = vec!["build", "--package", project];
    let mut target_path = "".to_owned();
    if let Some(t) = target {
        args.push("--target");
        args.push(t);
        target_path = format!("{}/", t);
    }

    if !debug {
        args.push("--release");
    }

    let mut dir = project_root();
    if let Some(subdir) = &directory {
        dir.push(subdir);
    }

    let status = Command::new(cargo())
        .current_dir(dir)
        .args(&args)
        .status()?;

    if !status.success() {
        return Err("cargo build failed".into());
    }

    Ok(project_root().join(&format!("target/{}{}/{}", target_path, stream, project)))
}

fn create_image(
    kernel: &Path,
    loader: &PathBuf,
    gateware: &PathBuf,
) -> Result<PathBuf, DynError> {
    let loader_bin_path = &format!("target/{}/release/loader.bin", TARGET);
    let kernel_bin_path = &format!("target/{}/release/kernel.bin", TARGET);
    // kernel region limit primarily set by the loader copy bytes. Can be grown, at expense of heap.
    const KERNEL_REGION: usize = 76 * 1024;
    // this is defined by size of UP5k bitstream plus rounding to sector erase size of 4k; reset vector points just beyond this
    const GATEWARE_REGION: usize = 104 * 1024;

    //let temp = loader.clone();
    //println!("attempt to assemble {:?}", temp.into_os_string());
    let loader_orig = loader.clone();
    let mut loader_elf = loader.clone();
    loader_elf.pop();
    loader_elf.push("loader.elf");
    // assemble the loader into an ELF file
    Command::new("riscv64-unknown-elf-as")
    .arg("-fpic")
    .arg(loader_orig.into_os_string())
    .arg("-o")
    .arg(loader_elf.into_os_string())
    .output()
    .expect("Failed to assemble the loader");

    // copy the ELF into a bin target
    let tmp = PathBuf::from(loader_bin_path);
    let mut loader_elf = loader.clone();
    loader_elf.pop();
    loader_elf.push("loader.elf");
    Command::new("riscv64-unknown-elf-objcopy")
    .arg("-O")
    .arg("binary")
    .arg(loader_elf.into_os_string())
    .arg(tmp.into_os_string())
    .output()
    .expect("Failed to copy loader binary");

    // extend the loader binary to 4096 bytes by padding with 0's
    let mut loader: [u8; 4096] = [0; 4096];
    std::fs::File::open(PathBuf::from(&loader_bin_path))?.read(&mut loader)?;
    std::fs::write(PathBuf::from(&loader_bin_path), loader)?;

    // objcopy the target sw into a binary format
    Command::new("riscv64-unknown-elf-objcopy")
    .arg("-O").arg("binary")
    .arg(kernel)
    .arg(PathBuf::from(&kernel_bin_path))
    .output()
    .expect("Failed to copy the kernel binary");

    // 104k region for gateware
    let mut gateware_bin: [u8; GATEWARE_REGION] = [0; GATEWARE_REGION];
    // kernel bin can be no longer than 48k, due to limitation on loader size
    let mut kernel_bin: [u8; KERNEL_REGION] = [0; KERNEL_REGION];

    std::fs::File::open(gateware)?.read(&mut gateware_bin)?;
    let kernel_bytes = std::fs::File::open(PathBuf::from(&kernel_bin_path))?.read(&mut kernel_bin);
    match kernel_bytes {
        Ok(bytes) => {
            println!("Read {} kernel bytes into image.", bytes);
            if bytes == KERNEL_REGION {
                println!("WARNING: kernel may be truncated.");
            }
        },
        _ => {
            println!("Error in reading kernel");
        }
    }

    let mut image = std::fs::File::create(PathBuf::from(&IMAGE_PATH))?;
    image.write(&gateware_bin)?;
    image.write(&loader)?;
    image.write(&kernel_bin)?;

    Ok(project_root().join(&IMAGE_PATH))
}

fn cargo() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}
