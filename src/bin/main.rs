extern crate config;

use std::net::TcpListener;
use std::env;
use std::path::Path;
use threadpool::ThreadPool;

use c20web::handle_connection;
use c20web::SETTINGS;
use c20web::DEFAULT_CONFIG;

fn main()
{
	let working_dir = Path::new("data");
	env::set_current_dir(&working_dir).expect("Couldn't set cwd");

	let (threads_max,listen_addr): (usize,String) = {
		let mut settings = SETTINGS.write().expect("Couldn't get config in main");
		settings.merge(config::File::from_str(&DEFAULT_CONFIG, config::FileFormat::Toml)).expect("Couldn't merge default config");
		settings.merge(config::File::with_name("web")).expect("Couldn't merge config from file");

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

