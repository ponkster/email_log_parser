use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::env;
use std::time::Instant;
use csv::WriterBuilder;
use encoding_rs::*;
use dotenv::dotenv;
use grep_searcher::{Searcher, SearcherBuilder};
use grep_matcher::LineTerminator;
use grep_regex::RegexMatcher;
use regex::RegexBuilder;
use std::io::Cursor;

// Custom matcher for ripgrep-style filtering
struct EmailFilter {
    exclude_domains: Vec<String>,
    max_email_length: usize,
    exclude_patterns: Vec<regex::Regex>,
}

impl EmailFilter {
    fn new(exclude_domains: Vec<String>) -> Result<Self, Box<dyn Error>> {
        // Compile regex patterns for unwanted content
        let exclude_patterns = vec![
            RegexBuilder::new(r"SendEmail-PreprocessPayload")
                .case_insensitive(true)
                .build()?,
            RegexBuilder::new(r"\.OUTLOOK\.COM>")
                .case_insensitive(true)
                .build()?,
            RegexBuilder::new(r"@odspnotify")
                .case_insensitive(true)
                .build()?,
            // Pattern for extremely long system-generated emails
            RegexBuilder::new(r"[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}")
                .case_insensitive(true)
                .build()?,
        ];

        Ok(EmailFilter {
            exclude_domains,
            max_email_length: 60, // Reasonable business email length
            exclude_patterns,
        })
    }

    fn should_exclude_line(&self, line: &str) -> bool {
        // Use ripgrep-style pattern matching to quickly exclude lines
        for pattern in &self.exclude_patterns {
            if pattern.is_match(line) {
                return true;
            }
        }
        false
    }

    fn should_exclude_email(&self, email: &str) -> bool {
        // Quick length check
        if email.len() > self.max_email_length {
            return true;
        }

        // Check domain exclusions
        for domain in &self.exclude_domains {
            if email.ends_with(domain) {
                return true;
            }
        }

        // Additional pattern checks for system emails
        for pattern in &self.exclude_patterns {
            if pattern.is_match(email) {
                return true;
            }
        }

        false
    }
}

// Custom searcher sink to collect matching lines
struct LineCollector {
    lines: Vec<String>,
    filter: EmailFilter,
}

impl LineCollector {
    fn new(filter: EmailFilter) -> Self {
        LineCollector {
            lines: Vec::new(),
            filter,
        }
    }
}

impl grep_searcher::Sink for LineCollector {
    type Error = Box<dyn Error>;

    fn matched(
        &mut self,
        _searcher: &Searcher,
        mat: &grep_searcher::SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        let line = std::str::from_utf8(mat.bytes())?;
        
        // Apply ripgrep-style filtering
        if !self.filter.should_exclude_line(line) {
            self.lines.push(line.to_string());
        }
        
        Ok(true) // Continue searching
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let start_time = Instant::now();
    
    // Load environment variables from .env file
    dotenv().ok();
    
    // Get configurable exclusion domains from environment variable
    let exclusion_domains_str = env::var("EXCLUSION_DOMAINS").unwrap_or_else(|_| "@ycp.com".to_string());
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
        eprintln!("  {} report2.csv mtsummary_output.csv", args[0]);
        std::process::exit(1);
    }
    
    let input_file = &args[1];
    let output_file = if args.len() >= 3 {
        &args[2]
    } else {
        "mtsummary_output.csv"
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

    // Debug: Show first few lines to understand the format
    let debug_lines: Vec<&str> = content.lines().take(3).collect();
    println!("Debug - First 3 lines of decoded content:");
    for (i, line) in debug_lines.iter().enumerate() {
        println!("Line {}: {}", i, &line[..std::cmp::min(line.len(), 100)]);
    }

    // Phase 2: Ripgrep Pre-filtering
    let ripgrep_start = Instant::now();
    
    // Initialize ripgrep-style filtering
    let email_filter = EmailFilter::new(exclusion_domains.clone())?;
    
    // Use ripgrep to pre-filter content for performance
    // Pattern to match lines starting with timestamps (data lines)
    let timestamp_matcher = RegexMatcher::new_line_matcher(r#"^"202[0-9]"#)?;
    
    let mut searcher = SearcherBuilder::new()
        .line_number(false)
        .line_terminator(LineTerminator::byte(b'\n'))
        .build();
    
    let mut line_collector = LineCollector::new(email_filter);
    
    // Search for timestamp lines and collect them
    // Convert to UTF-8 bytes for ripgrep processing
    let utf8_content = content.as_bytes();
    let cursor = Cursor::new(utf8_content);
    searcher.search_reader(&timestamp_matcher, cursor, &mut line_collector)?;
    
    let ripgrep_duration = ripgrep_start.elapsed();
    println!("⏱️  Ripgrep pre-filtering: {:.2}ms", ripgrep_duration.as_millis());
    println!("Ripgrep pre-filtering found {} potential data lines", line_collector.lines.len());
    
    // Fallback: if ripgrep didn't find anything, parse manually
    let fallback_start = Instant::now();
    let lines_to_process = if line_collector.lines.is_empty() {
        println!("Ripgrep found no matches, falling back to manual line filtering");
        content.lines()
            .skip(1) // Skip header
            .filter(|line| line.trim().starts_with("\"202"))
            .map(|s| s.to_string())
            .collect()
    } else {
        line_collector.lines
    };
    let fallback_duration = fallback_start.elapsed();
    
    if fallback_duration.as_millis() > 0 {
        println!("⏱️  Fallback filtering: {:.2}ms", fallback_duration.as_millis());
    }
    
    println!("Processing {} data lines", lines_to_process.len());
    
    // Phase 3: Data Processing and Output Generation
    let processing_start = Instant::now();
    
    let mut processed_count = 0;
    let mut seen_recipients = HashSet::new();
    let mut successful_records = 0;
    let mut error_count = 0;
    
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
    
    // Process the filtered lines
    for (line_num, line) in lines_to_process.iter().enumerate() {
        // Extract fields manually using basic parsing
        let parts: Vec<&str> = line.split("\",\"").collect();
        
        if parts.len() >= 6 {
            // Clean up the fields by removing quotes
            let timestamp = parts[0].trim_start_matches('"');
            let sender = parts[1];
            let recipient_status = parts[2];
            let subject = parts.get(3).map_or("", |v| v);
            
            // Find message ID (look for email-like pattern in angle brackets)
            let mut message_id = "";
            for part in &parts {
                if part.starts_with('<') && part.contains('@') {
                    message_id = part.trim_end_matches('"');
                    break;
                }
            }
            
            // Only process emails from the configured exclusion domains senders
            if !exclusion_domains.iter().any(|domain| sender.ends_with(domain)) {
                continue;
            }
            
            successful_records += 1;

            // Parse recipient_status to extract individual email addresses
            // Format: "email##status;email##status;..." (semicolon-separated)
            let recipients: Vec<&str> = recipient_status
                .split(';')
                .filter_map(|recipient_entry| {
                    // Split by "##" to separate email from status
                    let email = recipient_entry.split("##").next().unwrap_or("").trim();
                    // Remove any leading/trailing quotes and clean up
                    let clean_email = email.trim_matches('"').trim();
                    
                    // Use ripgrep-style filtering for valid emails
                    if clean_email.contains("@") {
                        // Apply email filter
                        let filter = EmailFilter::new(exclusion_domains.clone()).unwrap();
                        if !filter.should_exclude_email(clean_email) {
                            Some(clean_email)
                        } else {
                            None
                        }
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
                println!("Error parsing line {}: Insufficient fields", line_num + 1);
            }
        }
        
        // Show progress every 500 records for processed data (less frequent for performance)
        if (line_num + 1) % 500 == 0 {
            println!("Processed {} lines, {} successful records, {} unique recipients", 
                     line_num + 1, successful_records, processed_count);
        }
    }

    wtr.flush()?;
    let processing_duration = processing_start.elapsed();
    
    // Total time calculation
    let total_duration = start_time.elapsed();

    // Performance summary
    println!("\n📊 PERFORMANCE SUMMARY");
    println!("========================");
    println!("⏱️  File reading & encoding: {:.2}ms ({:.1}%)", 
             file_read_duration.as_millis(),
             (file_read_duration.as_millis() as f64 / total_duration.as_millis() as f64) * 100.0);
    println!("⏱️  Ripgrep pre-filtering:   {:.2}ms ({:.1}%)", 
             ripgrep_duration.as_millis(),
             (ripgrep_duration.as_millis() as f64 / total_duration.as_millis() as f64) * 100.0);
    if fallback_duration.as_millis() > 0 {
        println!("⏱️  Fallback filtering:      {:.2}ms ({:.1}%)", 
                 fallback_duration.as_millis(),
                 (fallback_duration.as_millis() as f64 / total_duration.as_millis() as f64) * 100.0);
    }
    println!("⏱️  Data processing:         {:.2}ms ({:.1}%)", 
             processing_duration.as_millis(),
             (processing_duration.as_millis() as f64 / total_duration.as_millis() as f64) * 100.0);
    println!("⏱️  TOTAL PROCESSING TIME:   {:.2}ms", total_duration.as_millis());
    println!("📈 Processing rate:          {:.0} records/second", 
             lines_to_process.len() as f64 / total_duration.as_secs_f64());

    println!("\n✅ RESULTS SUMMARY");
    println!("==================");
    println!("Data lines processed: {}", lines_to_process.len());
    println!("Total encoding/parse errors: {}", error_count);
    println!("Successful records: {}", successful_records);
    println!("Unique clean business emails found (excluding {}, system emails, and long addresses): {}", 
             exclusion_domains.join(", "), processed_count);
    Ok(())
}
