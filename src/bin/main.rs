use std::net::TcpListener;
use std::env;
use std::path::Path;
use threadpool::ThreadPool;

use c20web::handle_connection;

fn main()
{
	const MAX_WORKERS: usize = 100;

	let working_dir = Path::new("data");
	env::set_current_dir(&working_dir).unwrap();

    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
	let pool = ThreadPool::new(MAX_WORKERS);

    for stream in listener.incoming()
	{
		let stream = stream.unwrap();
        pool.execute(move ||{handle_connection(stream);});
    }
	println!("Shutting down.");
}

