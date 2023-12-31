use std::{env, process::exit, sync::Arc};

use anyhow::{Context, Error, format_err};
use futures03::StreamExt;
use prost::Message;

use pb::sf::substreams::rpc::v2::{BlockScopedData, BlockUndoSignal};
use pb::sf::substreams::v1::Package;
use substreams::SubstreamsEndpoint;
use substreams_stream::{BlockResponse, SubstreamsStream};
use crate::helper::token_request::request_token;

mod pb;
mod substreams;
mod substreams_stream;
mod helper;


#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = env::args();
    if args.len() != 4 {
        println!("usage: stream <endpoint> <spkg> <module>");
        println!();
        println!("The environment variable SUBSTREAMS_API_TOKEN must be set also");
        println!("and should contain a valid Substream API token.");
        exit(1);
    }

    let endpoint_url = env::args().nth(1).unwrap();
    let package_file = env::args().nth(2).unwrap();
    let module_name = env::args().nth(3).unwrap();

    let token: Option<String> = request_token(env::var("STREAMINGFAST_KEY").expect("please set env with: STREAMINGFAST_KEY")).await;
    let package = read_package(&package_file)?;
    let endpoint = Arc::new(SubstreamsEndpoint::new(&endpoint_url, token).await?);

    let cursor: Option<String> = load_persisted_cursor()?;

    let mut stream = SubstreamsStream::new(
        endpoint.clone(),
        cursor,
        package.modules.clone(),
        module_name.to_string(),
        // Start/stop block are not handled within this project, feel free to play with it
        0,
        0,
    );

    loop {
        match stream.next().await {
            None => {
                println!("Stream consumed");
                break;
            }
            Some(Ok(BlockResponse::New(data))) => {
                process_block_scoped_data(&data)?;
                persist_cursor(data.cursor)?;
            }
            Some(Ok(BlockResponse::Undo(undo_signal))) => {
                process_block_undo_signal(&undo_signal)?;
                persist_cursor(undo_signal.last_valid_cursor)?;
            }
            Some(Err(err)) => {
                println!();
                println!("Stream terminated with error");
                println!("{:?}", err);
                exit(1);
            }
        }
    }

    Ok(())
}

fn process_block_scoped_data(data: &BlockScopedData) -> Result<(), Error> {
    let output = data.output.as_ref().unwrap().map_output.as_ref().unwrap();

    // You can decode the actual Any type received using this code:
    //
    //     use prost::Message;
    //     let value = Message::decode::<GeneratedStructName>(data.value.as_slice())?;
    //
    // Where GeneratedStructName is the Rust code generated for the Protobuf representing
    // your type.

    println!(
        "Block #{} - Payload {} ({} bytes)",
        data.clock.as_ref().unwrap().number,
        output.type_url.replace("type.googleapis.com/", ""),
        output.value.len()
    );

    Ok(())
}

fn process_block_undo_signal(_undo_signal: &BlockUndoSignal) -> Result<(), anyhow::Error> {
    // `BlockUndoSignal` must be treated as "delete every data that has been recorded after
    // block height specified by block in BlockUndoSignal". In the example above, this means
    // you must delete changes done by `Block #7b` and `Block #6b`. The exact details depends
    // on your own logic. If for example all your added record contain a block number, a
    // simple way is to do `delete all records where block_num > 5` which is the block num
    // received in the `BlockUndoSignal` (this is true for append only records, so when only `INSERT` are allowed).
    unimplemented!("you must implement some kind of block undo handling, or request only final blocks (tweak substreams_stream.rs)")
}

fn persist_cursor(_cursor: String) -> Result<(), anyhow::Error> {
    // FIXME: Handling of the cursor is missing here. It should be saved each time
    // a full block has been correctly processed/persisted. The saving location
    // is your responsibility.
    //
    // By making it persistent, we ensure that if we crash, on startup we are
    // going to read it back from database and start back our SubstreamsStream
    // with it ensuring we are continuously streaming without ever losing a single
    // element.
    Ok(())
}

fn load_persisted_cursor() -> Result<Option<String>, anyhow::Error> {
    // FIXME: Handling of the cursor is missing here. It should be loaded from
    // somewhere (local file, database, cloud storage) and then `SubstreamStream` will
    // be able correctly resume from the right block.
    Ok(None)
}

fn read_package(file: &str) -> Result<Package, anyhow::Error> {
    let content = std::fs::read(file).context(format_err!("read package {}", file))?;
    Package::decode(content.as_ref()).context("decode command")
}
