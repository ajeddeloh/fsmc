extern crate ncurses;

use std::char;
use std::io::{BufReader, BufRead, Write};
use std::net::TcpStream;

use ncurses::*;

#[derive(Debug)]
struct MPDSearch {
    search_type: String,
    search_term: String,
}

impl MPDSearch {
    fn to_mpd_string(&self) -> String {
        format!("{} \"{}\" ", self.search_type, self.search_term)
    }
}

fn char_to_mpd_type(c: char) -> Option<String> {
    match c {
        ' ' => Some("any"),
        't' => Some("title"),
        'T' => Some("track"),
        'd' => Some("disc"),
        'b' => Some("album"),
        'a' => Some("artist"),
        'A' => Some("albumartist"),
        '\n' => Some(""), //end of input
        _ => None
    }.map(|x| x.to_string() )
}

fn is_valid_char(x: char) -> bool {
    x.is_alphanumeric() || x.is_whitespace()
}

fn get_valid_char() -> char {
    let c = char::from_u32(getch() as u32);
    match c {
        Some (x) if is_valid_char(x) => x,
        Some (_) => get_valid_char(),
        None => get_valid_char(),
    }
}

fn get_valid_type() -> String {
    match char_to_mpd_type(get_valid_char()) {
        Some(x) => x,
        None => get_valid_type(),
    }
}

fn do_mpd_update(constraints: &Vec<MPDSearch>, conn: &mut TcpStream, reader: &mut BufReader<TcpStream>) {
    let mut search_string = String::from("search ");
    for constraint in constraints {
        search_string.push_str(&constraint.to_mpd_string());
    }
    search_string.push('\n');
    conn.write(search_string.as_bytes()).ok().expect("DEATH AND SUFFERING");
    let mut matched_tracks: Vec<String> = Vec::new();
    let mut buffer = String::new();
    while !buffer.starts_with("OK") && !buffer.starts_with("ACK") {
        buffer.clear();
        reader.read_line(&mut buffer).ok().expect("Failed to read from mpd socket");
        if buffer.starts_with("file: ") {
            matched_tracks.push(String::from(&buffer[6..]));
        }
    }
    print_screen(&constraints, &matched_tracks);
}

fn print_screen(constraints: &Vec<MPDSearch>, files: &Vec<String>) {
    wclear(stdscr);
    addstr("Filters:\n");
    for constraint in constraints {
        addstr(&constraint.to_mpd_string());
        addch('\n' as u64);
    }
    for file in files {
        addstr(&file);
        addch('\n' as u64);
    }
}

fn main() {
    //connect to the mpd socket
    let read_conn = TcpStream::connect("127.0.0.1:6600").unwrap();
    let mut write_conn = read_conn.try_clone().ok().expect("ERROR");
    let mut reader = BufReader::new(read_conn);
    let mut buf = String::new();
    reader.read_line(&mut buf)
        .ok()
        .expect("Error reading line from MPD server");
    assert!( buf.starts_with("OK"), "Server did not return OK");

    //setup ncurses
    initscr();
    noecho();
    printw("Filters:\n");
    refresh();
    
    //create initial set of search constraints
    let mut constraints: Vec<MPDSearch> = Vec::new();

    loop {
        let current_search = MPDSearch { search_type: get_valid_type(), search_term: String::new() };
        if current_search.search_type == "" {
            break;
        }
        addstr(&current_search.search_type);
        constraints.push(current_search);
        let mut c = get_valid_char();
        while c != '\n' {
            addch(c as u64);
            constraints.last_mut().unwrap().search_term.push(c);
            refresh();

            do_mpd_update(&constraints, &mut write_conn, &mut reader);
            c = get_valid_char();
        }
        addch('\n' as u64);
        refresh();

    }   
    endwin();
    for constraint in &constraints {
        println!("{:?}", constraint);
    }
    println!("Added songs");
}
