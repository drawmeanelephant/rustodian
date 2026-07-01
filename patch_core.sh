sed -i '/mod tests {/r /dev/stdin' crates/rustodian-core/src/log_buffer.rs << 'INNER'

    #[test]
    fn test_log_buffer_exact_capacity() {
        let buf = LogBuffer::with_capacity(3);

        // Push exact capacity
        buf.push_line("1".to_string());
        buf.push_line("2".to_string());
        buf.push_line("3".to_string());

        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.snapshot(), "1\n2\n3\n");

        // Push one more, causing eviction
        buf.push_line("4".to_string());
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.snapshot(), "2\n3\n4\n");
    }
INNER
