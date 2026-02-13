use base64::engine::general_purpose::STANDARD as b64;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");

    match cmd {
        "keygen" => cmd_keygen(&args[2..]),
        "sign" => cmd_sign(&args[2..]),
        "verify" => cmd_verify(&args[2..]),
        _ => {
            eprintln!(
                "usage:\n  rzn-plugin-devkit keygen --out <dir>\n  rzn-plugin-devkit sign --key <ed25519.private> --input <plugin.json> --output <plugin.sig>\n  rzn-plugin-devkit verify --public <ed25519.public> --input <plugin.json> --sig <plugin.sig>"
            );
            std::process::exit(2);
        }
    }
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn cmd_keygen(args: &[String]) -> anyhow::Result<()> {
    let out_dir = arg_value(args, "--out").unwrap_or_else(|| "keys".to_string());
    let out = PathBuf::from(out_dir);
    std::fs::create_dir_all(&out)?;

    let signing = SigningKey::generate(&mut OsRng);
    let verify: VerifyingKey = signing.verifying_key();

    let priv_path = out.join("ed25519.private");
    let pub_path = out.join("ed25519.public");

    // Match the host expectations:
    // - private: base64 32-byte Ed25519 seed
    // - public: base64 32-byte Ed25519 verifying key
    std::fs::write(
        priv_path.clone(),
        format!("{}\n", b64.encode(signing.to_bytes())),
    )?;
    std::fs::write(
        pub_path.clone(),
        format!("{}\n", b64.encode(verify.to_bytes())),
    )?;

    println!("wrote {}", priv_path.display());
    println!("wrote {}", pub_path.display());
    Ok(())
}

fn read_b64_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    let s = std::fs::read_to_string(path)?.trim().to_string();
    Ok(b64.decode(s.as_bytes())?)
}

fn cmd_sign(args: &[String]) -> anyhow::Result<()> {
    let key_path = arg_value(args, "--key").ok_or_else(|| anyhow::anyhow!("missing --key"))?;
    let input_path =
        arg_value(args, "--input").ok_or_else(|| anyhow::anyhow!("missing --input"))?;
    let output_path =
        arg_value(args, "--output").ok_or_else(|| anyhow::anyhow!("missing --output"))?;

    let key_bytes = read_b64_file(Path::new(&key_path))?;
    if key_bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "invalid Ed25519 private key length: {} (expected 32)",
            key_bytes.len()
        ));
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&key_bytes);
    let signing = SigningKey::from_bytes(&seed);

    let message = std::fs::read(&input_path)?;
    let sig: Signature = signing.sign(&message);

    std::fs::write(output_path, format!("{}\n", b64.encode(sig.to_bytes())))?;
    Ok(())
}

fn cmd_verify(args: &[String]) -> anyhow::Result<()> {
    let public_path =
        arg_value(args, "--public").ok_or_else(|| anyhow::anyhow!("missing --public"))?;
    let input_path =
        arg_value(args, "--input").ok_or_else(|| anyhow::anyhow!("missing --input"))?;
    let sig_path = arg_value(args, "--sig").ok_or_else(|| anyhow::anyhow!("missing --sig"))?;

    let pk_bytes = read_b64_file(Path::new(&public_path))?;
    if pk_bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "invalid Ed25519 public key length: {} (expected 32)",
            pk_bytes.len()
        ));
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_bytes);
    let verifying = VerifyingKey::from_bytes(&pk_arr)?;

    let sig_bytes = read_b64_file(Path::new(&sig_path))?;
    if sig_bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "invalid Ed25519 signature length: {} (expected 64)",
            sig_bytes.len()
        ));
    }
    let sig = Signature::from_slice(&sig_bytes)?;

    let message = std::fs::read(&input_path)?;
    verifying.verify(&message, &sig)?;
    Ok(())
}
