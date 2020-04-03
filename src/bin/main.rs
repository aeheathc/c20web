extern crate clap;

use std::net::TcpListener;
use std::env;
use std::path::Path;
use threadpool::ThreadPool;
use clap::{Arg, App};

use c20web::handle_connection;
use c20web::SETTINGS;
use c20web::DEFAULT_CONFIG;

fn main()
{
    let matches = App::new("c20web")
                          .version("1.0.0-dev")
                          .about("Very simple web server")
                          .arg(Arg::with_name("working_dir")
                               .short("d")
                               .long("workingdir")
							   .help("Working directory. Will look here for the config file (web.toml) and will be the base for all relative paths used elsewhere in the config. Defaults to ./data for easy dev use with `cargo run` but an absolute path is recommended otherwise.")
							   .default_value("data")
                               .takes_value(true))
                          .get_matches();

	let working_dir = matches.value_of("working_dir").unwrap();
	env::set_current_dir(Path::new(working_dir)).expect("Couldn't set cwd");

	let (threads_max,listen_addr): (usize,String) = {
		let mut settings = SETTINGS.write().expect("Couldn't get config in main");
		settings.merge(config::File::from_str(&DEFAULT_CONFIG, config::FileFormat::Toml)).expect("Couldn't merge default config");
		settings.merge(config::File::with_name("web")).expect("Couldn't merge config from file");
		settings.set("working_dir",working_dir).expect("Couldn't merge config from commandline");

		(
			settings.get::<usize>("threads_max").expect("threads_max missing from config"),
			settings.get::<String>("listen_addr").expect("listen_addr missing from config:")
		)
	};

    let listener = TcpListener::bind(&listen_addr).expect(&(format!("Couldn't bind to addr: {}", &listen_addr)));
	let pool = ThreadPool::new(threads_max);

    for stream in listener.incoming()
	{
		let stream = stream.unwrap();
        pool.execute(move ||{handle_connection(stream);});
    }
	println!("Shutting down.");
}

