# Email Log Parser

A Rust-based email log parser designed to extract external recipient contacts from Microsoft Exchange email trace reports for CRM applications.

## Overview

This application processes Microsoft Exchange Message Trace reports downloaded from the [Microsoft Exchange Admin Center](https://admin.cloud.microsoft/exchange) and extracts clean business email addresses of external recipients. It's designed to help build CRM contact lists by identifying genuine business contacts while filtering out internal communications and system-generated emails.

## Features

- ✅ **Multi-encoding Support**: Handles UTF-8, UTF-16 Little Endian, and Windows-1252 encoded files
- ✅ **Configurable Domain Filtering**: Support for multiple exclusion domains via `.env` configuration
- ✅ **Smart Filtering**: Removes internal emails, Exchange system messages, and long system-generated addresses
- ✅ **Command Line Interface**: Flexible input/output file specification
- ✅ **Deduplication**: Automatically removes duplicate email addresses
- ✅ **Progress Reporting**: Shows processing progress and detailed statistics

## Installation

### Prerequisites
- Rust 1.87.0 or later
- Cargo package manager

### Setup
1. Clone or download this repository
2. Navigate to the project directory
3. Copy the environment configuration:
   ```bash
   cp .env.example .env
   ```
4. Build the application:
   ```bash
   cargo build --release
   ```

## Configuration

### Environment Variables
Configure the application by editing the `.env` file:

```env
# Exclusion domains - emails from/to these domains will be filtered appropriately
# Senders must be FROM one of these domains to be processed
# Recipients must NOT be TO any of these domains (external contacts only)
# Use comma-separated values for multiple domains
EXCLUSION_DOMAINS=@yourcompany.com

# Multiple domain examples:
# EXCLUSION_DOMAINS=@yourcompany.com,@subsidiary.com,@partner.org
# EXCLUSION_DOMAINS=@maincompany.com,@subsidiary1.com,@subsidiary2.net
```

### Filtering Logic
The application applies the following filters to extract clean business emails:

1. **Sender Validation**: Only processes emails FROM configured exclusion domains
2. **Recipient Filtering**: Excludes recipients TO configured exclusion domains (keeps external contacts only)
3. **Exchange System Filter**: Removes emails ending with `.OUTLOOK.COM>`
4. **Length Filter**: Excludes overly long email addresses (>60 characters, typically system-generated)
5. **Keyword Filter**: Removes emails containing system keywords like `SendEmail` and `PreprocessPayload`
6. **Deduplication**: Ensures unique email addresses in output

## Usage

### Command Line Interface
```bash
# Basic usage with default output file
cargo run -- <input_file.csv>

# Specify custom output file
cargo run -- <input_file.csv> <output_file.csv>

# Show usage help
cargo run
```

### Examples
```bash
# Process a Message Trace report with default output
cargo run -- "MTSummary_Message trace report - _2024-07-13T120000.000Z__example-trace-id.csv"

# Process with custom output filename
cargo run -- input_report.csv external_contacts.csv

# Multiple domain configuration example
# Edit .env: EXCLUSION_DOMAINS=@yourcompany.com,@subsidiary.net
cargo run -- email_trace.csv crm_contacts.csv
```

## Microsoft Exchange Message Trace Reports

### How to Download Reports
1. Go to [Microsoft Exchange Admin Center](https://admin.cloud.microsoft/exchange)
2. Navigate to **Mail flow** > **Message trace**
3. Configure your search criteria (date range, sender, recipient, etc.)
4. Export the results as CSV
5. Use the downloaded CSV file as input for this parser

### Expected Input Format
The parser expects Microsoft Exchange Message Trace CSV reports with the following structure:
- Headers with fields like `origin_timestamp_utc`, `sender_address`, `recipient_status`, `message_subject`, `message_id`
- Multiple encoding support (UTF-8, UTF-16 LE, Windows-1252)
- Complex CSV quoting and embedded line breaks are handled automatically

## Output Format

The application generates a clean CSV file with the following columns:
- `origin_timestamp_utc`: Timestamp of the email
- `message_id`: Unique message identifier
- `sender_email`: Email address of the sender
- `recipient_email`: External recipient email address
- `message_subject`: Subject line of the email

### Sample Output
```csv
origin_timestamp_utc,message_id,sender_email,recipient_email,message_subject
2024-07-01T08:28:11.4671965Z,<MSG123ABC@exchange.yourcompany.com>,john.doe@yourcompany.com,contact@client-corp.com,Re: Confirmation of project status
2024-05-15T22:00:39.0330820Z,<MSG456DEF@exchange.yourcompany.com>,jane.smith@yourcompany.com,info@business-partner.co.jp,Project proposal discussion
```

## Performance

- **Processing Speed**: Handles large files efficiently with progress reporting every 1,000 records
- **Memory Usage**: Optimized for large CSV files with streaming processing
- **Error Handling**: Graceful handling of encoding issues and malformed CSV lines
- **Success Rate**: Typically achieves 95-100% success rate on well-formed Exchange reports

### Recent Test Results
```
File: Sample Message trace report (1.08MB)
Total Lines: 1,370 processed
Success Rate: 100% (0 errors)
External Contacts Found: 330 unique business emails
Processing Time: < 1 second
```

## Development

### Dependencies
- `csv = "1.3.1"` - CSV parsing and writing
- `encoding_rs = "0.8.35"` - Multi-encoding support
- `dotenv = "0.15.0"` - Environment variable loading

### Project Structure
```
├── src/
│   └── main.rs          # Main application logic
├── Cargo.toml           # Rust dependencies
├── .env                 # Environment configuration (not in git)
├── .env.example         # Configuration template
├── .gitignore           # Git ignore rules
└── README.md            # This file
```

### Building for Production
```bash
# Build optimized release version
cargo build --release

# Run optimized version
./target/release/email_logparser input.csv output.csv
```

## Use Cases

### CRM Integration
- **Lead Generation**: Extract external business contacts from email communications
- **Contact Discovery**: Identify new business relationships and partnerships
- **Data Cleaning**: Remove internal and system emails for clean contact lists

### Business Intelligence
- **Communication Analysis**: Understand external communication patterns
- **Relationship Mapping**: Identify key external contacts and organizations
- **Compliance**: Track external communications for regulatory purposes

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test with sample Exchange reports
5. Submit a pull request

## License

This project is provided as-is for internal business use. Ensure compliance with your organization's data handling policies when processing email data.

## Support

For issues related to:
- **Exchange Report Format**: Verify the CSV was downloaded correctly from Exchange Admin
- **Encoding Issues**: The parser supports multiple encodings automatically
- **Configuration**: Check your `.env` file format and domain specifications
- **Performance**: Large files (>10MB) may require additional memory

## Changelog

- **v0.1.0**: Initial release with basic filtering
- **v0.2.0**: Added multi-domain support and command line arguments
- **v0.3.0**: Added multi-encoding support and improved error handling 