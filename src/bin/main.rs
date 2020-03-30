use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::Arc;
use std::env;
use std::path::Path;
use threadpool::ThreadPool;

use c20web::http_response_table;
use c20web::handle_connection;

fn main()
{
	const MAX_WORKERS: usize = 100;

	let working_dir = Path::new("data");
	env::set_current_dir(&working_dir).unwrap();

	let http_codes = Arc::<HashMap<u16,String>>::new(http_response_table());
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
	let pool = ThreadPool::new(MAX_WORKERS);

    for stream in listener.incoming()
	{
		let stream = stream.unwrap();
		let http_codes_newarc = http_codes.clone();
        pool.execute(move ||{handle_connection(stream, http_codes_newarc);});
    }
	println!("Shutting down.");
}

