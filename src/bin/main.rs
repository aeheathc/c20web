extern crate clap;

use std::net::TcpListener;
use std::env;
use std::path::Path;
use std::process;
use clap::{Arg, App};
use log::{error, info};
use log4rs;
use threadpool::ThreadPool;

use c20web::handle_connection;
use c20web::statics::SETTINGS;
use c20web::statics::DEFAULT_CONFIG;

/**
Load configuration, set the working directory, initialize logging, and start listening for connections.

# Panics
If anything goes seriously wrong setting up config/cwd/logging, we panic.
*/
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

	log4rs::init_file("log4rs.yml", Default::default()).expect("log4rs.yml not found");
	//at this point the loggers are available and any further errors can be logged instead of bring thrown into a panic
	
	info!("Starting up.");
	start_listening(listen_addr, threads_max);
	info!("Shutting down.");
}

/**
Listen on an interface and start the main loop which accepts each new
connection and sends it to its own thread.

# Parameters
- `listen_addr`: The interface on which to listen
- `threads_max`: Size of the thread pool

# Examples
```no_run
start_listening(String::from("127.0.0.1:8000"), 100);
```

# Aborts
Will abort when unable to bind to the listen_addr
*/
fn start_listening(listen_addr: String, threads_max: usize)
{
	let listener = match TcpListener::bind(&listen_addr)
	{
		Ok(r) => r,
		Err(e) => {
			error!("Couldn't bind to addr {}: {}", &listen_addr, e);
			process::exit(1);
		}
	};
	let pool = ThreadPool::new(threads_max);

    for stream in listener.incoming()
	{
		let stream = match stream
		{
			Ok(s) => s,
			Err(e) =>{
				error!("Listener gave us an invalid TCPStream!: {}",e);
				continue;
			}
		};
        pool.execute(move ||{handle_connection(stream);});
    }
}