use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::env;
use std::time::Instant;
use csv::WriterBuilder;
use encoding_rs::*;
use dotenv::dotenv;

fn main() -> Result<(), Box<dyn Error>> {
    let start_time = Instant::now();
    
    // Load environment variables from .env file
    dotenv().ok();
    
    // Get configurable exclusion domains from environment variable
    let exclusion_domains_str = env::var("EXCLUSION_DOMAINS").unwrap_or_else(|_| "@mycompany.com".to_string());
    let exclusion_domains: Vec<String> = exclusion_domains_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    println!("Using exclusion domains: {:?}", exclusion_domains);
    
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <input_file.csv> [output_file.csv]", args[0]);
        eprintln!("Examples:");
        eprintln!("  {} input.csv", args[0]);
        eprintln!("  {} report2.csv filtered_emails.csv", args[0]);
        std::process::exit(1);
    }
    
    let input_file = &args[1];
    let output_file = if args.len() >= 3 {
        &args[2]
    } else {
        "filtered_emails.csv"
    };
    
    println!("Processing: {} -> {}", input_file, output_file);

    // Phase 1: File Reading and Encoding Detection
    let file_read_start = Instant::now();
    let mut file = File::open(input_file)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    
    // Try UTF-8 first, then UTF-16, then fallback to Windows-1252 if needed
    let content = match String::from_utf8(buffer.clone()) {
        Ok(utf8_content) => {
            println!("Using UTF-8 encoding");
            utf8_content
        },
        Err(_) => {
            // Try UTF-16 Little Endian
            if buffer.len() % 2 == 0 {
                let utf16_bytes: Vec<u16> = buffer
                    .chunks_exact(2)
                    .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();
                
                match String::from_utf16(&utf16_bytes) {
                    Ok(utf16_content) => {
                        println!("Using UTF-16 Little Endian encoding");
                        utf16_content
                    },
                    Err(_) => {
                        println!("UTF-16 failed, trying Windows-1252 encoding");
                        let (decoded_content, _encoding_used, had_errors) = WINDOWS_1252.decode(&buffer);
                        if had_errors {
                            println!("Warning: Some encoding errors were encountered and corrected");
                        }
                        decoded_content.to_string()
                    }
                }
            } else {
                println!("Buffer length not even, trying Windows-1252 encoding");
                let (decoded_content, _encoding_used, had_errors) = WINDOWS_1252.decode(&buffer);
                if had_errors {
                    println!("Warning: Some encoding errors were encountered and corrected");
                }
                decoded_content.to_string()
            }
        }
    };

    let file_read_duration = file_read_start.elapsed();
    println!("⏱️  File reading and encoding: {:.2}ms", file_read_duration.as_millis());
    
    // Phase 2: Line Parsing and Pre-processing
    let line_parsing_start = Instant::now();
    
    // Split content into lines and manually parse
    let lines: Vec<&str> = content.lines().collect();
    let total_input_lines = lines.len();
    
    let line_parsing_duration = line_parsing_start.elapsed();
    println!("⏱️  Line parsing and setup: {:.2}ms", line_parsing_duration.as_millis());
    
    // Phase 3: Data Processing and Filtering (Main Processing Loop)
    let processing_start = Instant::now();
    
    let mut processed_count = 0;
    let mut seen_recipients = HashSet::new();
    let mut successful_records = 0;
    let mut error_count = 0;
    let mut total_lines = 0;
    
    // Prepare CSV writer
    let output = File::create(output_file)?;
    let mut wtr = WriterBuilder::new()
        .from_writer(output);

    // Write headers
    wtr.write_record([
        "origin_timestamp_utc",
        "message_id",
        "sender_email",
        "recipient_email",
        "message_subject",
    ])?;
    
    let mut i = 1; // Skip header line
    
    while i < lines.len() {
        total_lines += 1;
        
        // Look for complete records by finding timestamp pattern
        let line = lines[i].trim();
        
        // Skip empty lines
        if line.is_empty() {
            i += 1;
            continue;
        }
        
        // Look for lines starting with timestamp pattern "2025-
        if line.starts_with("\"2025-") {
            // Extract fields manually using basic parsing
            let parts: Vec<&str> = line.split("\",\"").collect();
            
            if parts.len() >= 6 {
                // Clean up the fields by removing quotes
                let timestamp = parts[0].trim_start_matches('"');
                let sender = parts[1];
                let recipient_status = parts[2];
                let subject = parts.get(3).map_or("", |v| v);
                // Message ID might be in different positions due to embedded commas
                let mut message_id = "";
                for part in &parts {
                    if part.starts_with('<') && part.contains('@') {
                        message_id = part.trim_end_matches('"');
                        break;
                    }
                }
                
                // Only process emails from the configured exclusion domains senders
                if !exclusion_domains.iter().any(|domain| sender.ends_with(domain)) {
                    i += 1;
                    continue;
                }
                
                successful_records += 1;

                // Parse recipient_status to extract individual email addresses
                // Format: "email##status;email##status;..." (semicolon-separated, with some comma-separated status info)
                let recipients: Vec<&str> = recipient_status
                    .split(';')
                    .filter_map(|recipient_entry| {
                        // Split by "##" to separate email from status
                        let email = recipient_entry.split("##").next().unwrap_or("").trim();
                        // Remove any leading/trailing quotes and clean up
                        let clean_email = email.trim_matches('"').trim();
                        
                        // Only include valid emails that:
                        // 1. Contain @ symbol
                        // 2. Do NOT have the configured exclusion domain 
                        // 3. Do NOT end with .OUTLOOK.COM>
                        // 4. Are not excessively long (typical business emails are under 60 chars)
                        // 5. Do not contain system-generated keywords
                        if clean_email.contains("@") && 
                           !exclusion_domains.iter().any(|domain| clean_email.ends_with(domain)) && 
                           !clean_email.ends_with(".OUTLOOK.COM>") &&
                           clean_email.len() <= 60 &&
                           !clean_email.contains("SendEmail") &&
                           !clean_email.contains("PreprocessPayload") {
                            Some(clean_email)
                        } else {
                            None
                        }
                    })
                    .collect();

                // Write unique recipients to output
                for recipient in recipients {
                    if seen_recipients.insert(recipient.to_string()) {
                        wtr.write_record([
                            timestamp,
                            message_id,
                            sender,
                            recipient,
                            subject,
                        ])?;
                        processed_count += 1;
                    }
                }
            } else {
                error_count += 1;
                if error_count <= 10 {  // Only show first 10 errors
                    println!("Error parsing line {}: Insufficient fields", total_lines);
                }
            }
        } else {
            // Skip non-data lines (could be continuation lines)
        }
        
        // Show progress every 500 records (less frequent for performance)
        if total_lines % 500 == 0 {
            println!("Processed {} lines, {} successful records, {} unique recipients", 
                     total_lines, successful_records, processed_count);
        }
        
        i += 1;
    }

    wtr.flush()?;
    let processing_duration = processing_start.elapsed();
    
    // Total time calculation
    let total_duration = start_time.elapsed();

    // Performance summary
    println!("\n📊 PERFORMANCE SUMMARY (NON-RIPGREP VERSION)");
    println!("=============================================");
    println!("⏱️  File reading & encoding: {:.2}ms ({:.1}%)", 
             file_read_duration.as_millis(),
             (file_read_duration.as_millis() as f64 / total_duration.as_millis() as f64) * 100.0);
    println!("⏱️  Line parsing & setup:    {:.2}ms ({:.1}%)", 
             line_parsing_duration.as_millis(),
             (line_parsing_duration.as_millis() as f64 / total_duration.as_millis() as f64) * 100.0);
    println!("⏱️  Data processing:         {:.2}ms ({:.1}%)", 
             processing_duration.as_millis(),
             (processing_duration.as_millis() as f64 / total_duration.as_millis() as f64) * 100.0);
    println!("⏱️  TOTAL PROCESSING TIME:   {:.2}ms", total_duration.as_millis());
    println!("📈 Processing rate:          {:.0} records/second", 
             total_lines as f64 / total_duration.as_secs_f64());

    println!("\n✅ RESULTS SUMMARY");
    println!("==================");
    println!("Total input lines: {}", total_input_lines);
    println!("Data lines processed: {}", total_lines);
    println!("Total encoding/parse errors: {}", error_count);
    println!("Successful records: {}", successful_records);
    println!("Unique clean business emails found (excluding {}, Exchange IDs, and long system emails): {}", exclusion_domains.join(", "), processed_count);
    Ok(())
}
