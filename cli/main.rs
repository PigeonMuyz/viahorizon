use std::{path::Path, time::Duration};
use viahorizon_native::{self as vfp, Config, Selection};

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("versions") if args.len() == 1 => {
            println!("Horizon client: 26.3 (777). Target catalog is not gameplay support.");
            for r in vfp::RELEASES.iter().rev() { println!("{:<17} protocol {:<3} {:<7} {}",r.label(),r.protocol,if r.legacy {"legacy"} else {"netty"},r.stage.label()); }
        }
        Some("probe") if args.len() == 2 => {
            let s = vfp::probe(&args[1],Duration::from_secs(3))?;
            println!("Server: {} | protocol {} | {} ms",s.name,s.protocol,s.latency_ms);
            println!("Port status: {}",vfp::modern_protocol(s.protocol).map_or("unknown Release protocol",|r|r.stage.label()));
        }
        Some("target") if args.len() == 3 || args.len() == 4 => {
            let path = Path::new(&args[1]); let mut config = Config::load(path)?;
            let selection = Selection::parse(&args[2])?;
            if args.len() == 4 { config.servers.insert(vfp::server_key(&args[3])?,selection); } else { config.target = selection; }
            config.save(path)?; print!("{}",config.json());
        }
        _ => return Err("Usage: vh-native versions | probe HOST[:PORT] | target CONFIG_PATH native|auto|RELEASE [SERVER]\nThe standalone probe uses A/AAAA records; in-game auto detection also uses SRV.".into()),
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
