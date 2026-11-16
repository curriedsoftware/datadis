/***
 * Copyright (c) 2026 Rafael Fernández López <ereslibre@curried.software>
 *
 * Permission is hereby granted, free of charge, to any person
 * obtaining a copy of this software and associated documentation
 * files (the "Software"), to deal in the Software without
 * restriction, including without limitation the rights to use, copy,
 * modify, merge, publish, distribute, sublicense, and/or sell copies
 * of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
 * NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS
 * BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
 * ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 ***/

use clap::{Parser, Subcommand};
use datadis::api::{Auth, Client, SuppliesQuery};
use prettytable::{Table, row};

#[derive(Debug, Subcommand)]
enum RootCommand {
    Supplies,
    Contracts,
    Consumption,
    MaxPower,
    Distributors,
    ReactiveData,
    Groups,
    Authorizations,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: RootCommand,
    #[arg(short = 'u', long = "username", env = "DATADIS_USERNAME")]
    username: String,
    #[arg(short = 'p', long = "password", env = "DATADIS_PASSWORD")]
    password: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let client = Client::new(Auth {
        username: cli.username,
        password: cli.password,
    });
    match cli.command {
        RootCommand::Supplies => match client.supplies(SuppliesQuery::default()).await {
            Ok(supplies) => {
                let mut table = Table::new();
                table.set_titles(row![
                    "CUPS",
                    "Address",
                    "Distributor",
                    "Valid from",
                    "Valid to",
                    "Point type",
                ]);
                for supply in &supplies.supplies {
                    table.add_row(row![
                        supply.cups,
                        supply.address,
                        supply.distributor,
                        supply.valid_date_from,
                        supply
                            .valid_date_to
                            .map(|date| date.to_string())
                            .unwrap_or_default(),
                        supply.point_type,
                    ]);
                }
                table.printstd();
                if !supplies.distributor_error.is_empty() {
                    eprintln!(
                        "\n⚠ {} distributor(s) returned errors:",
                        supplies.distributor_error.len()
                    );
                    let mut errors = Table::new();
                    errors.set_titles(row!["Distributor", "Error code", "Description",]);
                    for error in &supplies.distributor_error {
                        errors.add_row(row![
                            error.distributor_name,
                            error.error_code,
                            error.error_description,
                        ]);
                    }
                    eprint!("{errors}");
                }
            }
            Err(error) => eprintln!("could not list supplies: {error:?}"),
        },
        _ => eprintln!("command not implemented yet"),
    }
}
