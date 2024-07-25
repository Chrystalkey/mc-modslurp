use anyhow::Result;
use clap::Parser;
use serde::Deserialize;
use sevenz_rust::{self, SevenZWriter};
use std::fs;
use std::{collections::HashMap, path::PathBuf};

#[derive(Parser, Debug)]
struct InputArguments {
    config: String,
    #[arg(long, short)]
    compress: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Mod {
    link: String,
    server: bool,
    client: bool,
    depends_on : Option<Vec<String>>
}
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Info {
    name: String,
    mc_version: String,
    loader: String,
    launcher: String,
}
#[derive(Debug, Deserialize)]
struct ConfigFile {
    mods: HashMap<String, Mod>,
    info: Info,
}

fn download_file(
    link: String,
    output_directory: &PathBuf,
    file: Option<String>,
) -> Result<PathBuf> {
    println!("Downloading file from {}", link);
    let response = reqwest::blocking::get(link).unwrap();
    let filepath = if let Some(name) = file {
        name
    } else {
        response
            .url()
            .path_segments()
            .and_then(|segments| segments.last())
            .and_then(|name| if name.is_empty() { None } else { Some(name) })
            .unwrap_or("tmp.bin")
            .to_owned()
    };
    let filepath = output_directory.join(filepath);
    println!("Saving file to {}", filepath.display());
    let mut dest = fs::File::create(&filepath)?;
    let content = response.bytes()?;
    std::io::copy(&mut content.as_ref(), &mut dest)?;
    Ok(filepath)
}
fn compress(path: &PathBuf) -> Result<()> {
    // zip the server and client folders
    let mut zip = SevenZWriter::create(path.parent().unwrap().join(
        format!("{}.7z", path.file_name().unwrap().to_os_string().into_string().unwrap()).as_str(),
    ))?;
    zip.push_source_path(path, |_| true)?;
    zip.finish()?;

    Ok(())
}

fn main() -> Result<()> {
    let parsed_input = InputArguments::parse();
    println!("{:?}", parsed_input);
    let config_file: ConfigFile = toml::from_str(
        std::fs::read_to_string(&parsed_input.config)
            .unwrap()
            .as_str(),
    )
    .unwrap();
    println!("{:?}", config_file);
    let config_path = PathBuf::from(parsed_input.config);
    let out_dir = config_path.parent().unwrap().join(config_file.info.name);
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)?;
    }
    println!("Creating output directory");
    std::fs::create_dir_all(&out_dir)?;
    // download mods
    println!("Downloading mods into mods folder");
    fs::create_dir_all(out_dir.join("server/mods"))?;
    fs::create_dir_all(out_dir.join("client/mods"))?;
    fs::create_dir_all(out_dir.join("download"))?;
    for (modname, modstruct) in config_file.mods.iter() {
        println!("Downloading {}", modname);
        let fp = download_file(modstruct.link.clone(), &out_dir.join("download"), None)?;
        if modstruct.server {
            fs::copy(
                &fp,
                out_dir.join("server/mods").join(fp.file_name().unwrap()),
            )?;
        }
        if modstruct.client {
            fs::rename(
                &fp,
                out_dir.join("client/mods").join(fp.file_name().unwrap()),
            )?;
        }
    }
    fs::remove_dir_all(out_dir.join("download"))?;
    println!("Accepting EuLA");
    fs::write(out_dir.join("server").join("eula.txt"), "eula=true")?;
    let copied = out_dir.clone();
    if parsed_input.compress.unwrap_or(false) {
        let thread = std::thread::spawn(move || {
            compress(&copied.join("server")).unwrap();
        });
        compress(&out_dir.join("client"))?;
        thread.join().unwrap();
    }
    // initialize the server
    println!("Initializing server");
    let fabric_link = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/{}/server/jar",
        config_file.info.mc_version, config_file.info.loader, config_file.info.launcher
    );
    download_file(
        fabric_link,
        &out_dir.join("server"),
        Some("fabric.jar".to_owned()),
    )?;
    let result = std::process::Command::new("java")
        .current_dir(out_dir.join("server").display().to_string().as_str())
        .arg("-Xmx2G")
        .arg("-jar")
        .arg("fabric.jar")
        .arg("nogui")
        .arg("server")
        .output()?;
    println!("Exit Code: {}", result.status);
    println!("{}", String::from_utf8_lossy(&result.stdout));
    println!("{}", String::from_utf8_lossy(&result.stderr));
    println!("Server initialized");

    Ok(())
}
