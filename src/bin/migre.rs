use async_std::{fs::File, io, task};
use futures_util::io::AsyncReadExt;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "migre",
    about = "Proxy incoming tcp traffic and broadcast to all registered upstream"
)]
struct CLIArg {
    #[structopt(default_value = "/etc/migre/conf.toml", short, long)]
    config: String,
}

fn main() -> io::Result<()> {
    let opt = CLIArg::from_args();
    task::block_on(async {
        let mut conf_file = match File::open(&opt.config).await {
            Err(err) => panic!("Couldn't open {}: {}", opt.config, err.to_string()),
            Ok(file) => file,
        };
        let mut conf_str = String::new();
        match conf_file.read_to_string(&mut conf_str).await {
            Err(err) => panic!("couldn't read {}: {}", opt.config, err.to_string()),
            Ok(_) => {
                let config = migre::Config::from_config_str(&conf_str);
                migre::start(config).await;
            }
        }
    });

    Ok(())
}
