#[macro_use]
extern crate lazy_static;

use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;
use std::net::Shutdown;
use std::net::TcpStream;
use std::sync::RwLock;
use config::Config;

pub fn handle_connection(mut stream: TcpStream)
{
	println!("Starting to process request.");
	let settings = SETTINGS.read().expect("Couldn't get config in request thread");
	let webroot = settings.get::<String>("webroot").expect("webroot missing from config");
	let request_max_bytes = settings.get::<usize>("request_max_bytes").expect("request_max_bytes missing from config");

	println!("Creating buffer");
	let mut buffer = vec![0u8; request_max_bytes].into_boxed_slice();
	println!("Buffer created. Reading input");
	let request_result = stream.read(&mut buffer);
	println!("Request read. Starting analysis");
	let (response_code, mut response_body): (u16, String) = match request_result
	{
		Ok(num_bytes) => {
			if num_bytes >= request_max_bytes
			{
				(413,String::from(""))
			}else{
				let mut index_end_method = 0;
				let mut index_end_resource = 0;
				let mut index_end_line = 0;
				for (index, request_byte) in buffer.iter().enumerate()
				{
					if *request_byte == b'\r' || *request_byte == b'\n'
					{
						index_end_line = index;
						break;
					}else if *request_byte == b' '{
						if index_end_method == 0 {index_end_method = index;}
						else if index_end_resource == 0 {index_end_resource = index;}
					}
				}

				if index_end_line == 0 || index_end_resource == 0 || index_end_method == 0
				{
					(400, String::from("Malformed request line"))
				}else{
					let method: &[u8] = &(buffer[0..index_end_method]);
					let resource: &[u8] = &(buffer[(index_end_method+1)..index_end_resource]);
					let http_version: &[u8] = &(buffer[(index_end_resource+1)..index_end_line]);

					let resource_parse_res = std::str::from_utf8(resource);

					if method != b"GET"
					{
						(501,String::from(""))
					}else if http_version != b"HTTP/1.1"{
						(505,String::from(""))
					}else if let Err(res_err) = resource_parse_res{
						(400,format!("Malformed resource name: {}",res_err))
					}else{
						match resource_parse_res
						{
							Err(res_err) => (400,format!("Malformed resource name: {}", res_err)),
							Ok(resource) =>
							{
								let resource = resource.replacen(&"/",&"",1);
								let resource = format!("{}/{}", webroot, resource);
								println!("Requesting page: {}",&resource);
								let body_result = fs::read_to_string(&resource);
								match body_result
								{
									Err(read_err) => (404,format!("{}",read_err)),
									Ok(body) => (200,body)
								}
							}
						}
					}
				}
			}
		},
		Err(err_str) => {(400,format!("The network stream didn't stay valid long enough for the server to read it: {}",err_str))}
	};
	println!("Request analyzed. Closing input and starting output.");

	/* Any output won't make it to the browser if there is still input left to be read.
	 * In order to avoid DoS attacks by enforcing max request size, and still
	 * send the appropriate error message back, we need to discard the rest of
	 * the input without actually reading it in. Even calling shutdown on Read doesn't
	 * always do this but there doesn't seem to be any better way.
	*/
	let _shutdown_res = stream.shutdown(Shutdown::Read);

	let status = if let Some(status_str) = HTTP_RESPONSE_TABLE.get(&response_code)
	{
		format!("{} {}",response_code,status_str)
	}else{
		format!("{} Unknown",response_code)
	};

	if response_code < 200 || response_code >= 300
	{
		let errpage_result = fs::read_to_string("error.html");
		let mut error_page = match errpage_result
		{
			Err(_) => String::from("<!DOCTYPE html><html lang='en'><head><meta charset='utf-8'><title>{}</title></head><body><h1>{}</h1><p>{}</p></body></html>"),
			Ok(body) => body
		};
		error_page = error_page.replacen("{}", &status, 2);
		response_body = error_page.replacen("{}", &response_body, 1);
	}


	let headers = format!("Content-Type: text/html;\r\nContent-Length: {};", response_body.len());
	let response = format!("HTTP/1.1 {}\r\n{}\r\n\r\n{}", status, headers, response_body);
	
	let write_res = stream.write(response.as_bytes());
	match write_res
	{
		Ok(_) => {},
		Err(em) => {println!("Write error: {}",em);}
	}
	
	let flush_res = stream.flush();
	match flush_res
	{
		Ok(_) => {},
		Err(em) => {println!("Flush error: {}",em);}
	}
}

lazy_static!
{
	pub static ref DEFAULT_CONFIG: String = String::from("listen_addr = \"127.0.0.1:7878\"\nworking_dir = \"data\"\nwebroot = \"webroot\"\nthreads_max = 100\nrequest_max_bytes = 1000");

	pub static ref SETTINGS: RwLock<Config> = RwLock::new(Config::default());

    pub static ref HTTP_RESPONSE_TABLE: HashMap<u16,String> = {
        let mut codes = HashMap::<u16,String>::new();
        codes.insert(100, String::from("Continue"));
        codes.insert(101, String::from("Switching Protocols"));
        codes.insert(102, String::from("Processing"));
        codes.insert(200, String::from("OK"));
        codes.insert(201, String::from("Created"));
        codes.insert(202, String::from("Accepted"));
        codes.insert(203, String::from("Non-authoritative Information"));
        codes.insert(204, String::from("No Content"));
        codes.insert(205, String::from("Reset Content"));
        codes.insert(206, String::from("Partial Content"));
        codes.insert(207, String::from("Multi-Status"));
        codes.insert(208, String::from("Already Reported"));
        codes.insert(226, String::from("IM Used"));
        codes.insert(300, String::from("Multiple Choices"));
        codes.insert(301, String::from("Moved Permanently"));
        codes.insert(302, String::from("Found"));
        codes.insert(303, String::from("See Other"));
        codes.insert(304, String::from("Not Modified"));
        codes.insert(305, String::from("Use Proxy"));
        codes.insert(307, String::from("Temporary Redirect"));
        codes.insert(308, String::from("Permanent Redirect"));
        codes.insert(400, String::from("Bad Request"));
        codes.insert(401, String::from("Unauthorized"));
        codes.insert(402, String::from("Payment Required"));
        codes.insert(403, String::from("Forbidden"));
        codes.insert(404, String::from("Not Found"));
        codes.insert(405, String::from("Method Not Allowed"));
        codes.insert(406, String::from("Not Acceptable"));
        codes.insert(407, String::from("Proxy Authentication Required"));
        codes.insert(408, String::from("Request Timeout"));
        codes.insert(409, String::from("Conflict"));
        codes.insert(410, String::from("Gone"));
        codes.insert(411, String::from("Length Required"));
        codes.insert(412, String::from("Precondition Failed"));
        codes.insert(413, String::from("Payload Too Large"));
        codes.insert(414, String::from("Request-URI Too Long"));
        codes.insert(415, String::from("Unsupported Media Type"));
        codes.insert(416, String::from("Requested Range Not Satisfiable"));
        codes.insert(417, String::from("Expectation Failed"));
        codes.insert(418, String::from("I'm a teapot"));
        codes.insert(421, String::from("Misdirected Request"));
        codes.insert(422, String::from("Unprocessable Entity"));
        codes.insert(423, String::from("Locked"));
        codes.insert(424, String::from("Failed Dependency"));
        codes.insert(426, String::from("Upgrade Required"));
        codes.insert(428, String::from("Precondition Required"));
        codes.insert(429, String::from("Too Many Requests"));
        codes.insert(431, String::from("Request Header Fields Too Large"));
        codes.insert(444, String::from("Connection Closed Without Response"));
        codes.insert(451, String::from("Unavailable For Legal Reasons"));
        codes.insert(499, String::from("Client Closed Request"));
        codes.insert(500, String::from("Internal Server Error"));
        codes.insert(501, String::from("Not Implemented"));
        codes.insert(502, String::from("Bad Gateway"));
        codes.insert(503, String::from("Service Unavailable"));
        codes.insert(504, String::from("Gateway Timeout"));
        codes.insert(505, String::from("HTTP Version Not Supported"));
        codes.insert(506, String::from("Variant Also Negotiates"));
        codes.insert(507, String::from("Insufficient Storage"));
        codes.insert(508, String::from("Loop Detected"));
        codes.insert(510, String::from("Not Extended"));
        codes.insert(511, String::from("Network Authentication Required"));
        codes.insert(599, String::from("Network Connect Timeout Error"));
        codes
    };
}