// INPUT:  (none)
// OUTPUT: format_i32(), format_progress()
// POS:    no_std 格式化工具函数，纯函数无外部依赖

/// 格式化 i32 到固定缓冲区，返回字符串切片
pub fn format_i32(value: i32, buf: &mut [u8; 16]) -> &str {
    let mut pos = buf.len();
    let negative = value < 0;
    let mut v = if negative {
        (value as i64).unsigned_abs()
    } else {
        value as u64
    };

    if v == 0 {
        pos -= 1;
        buf[pos] = b'0';
    } else {
        while v > 0 {
            pos -= 1;
            buf[pos] = b'0' + (v % 10) as u8;
            v /= 10;
        }
    }

    if negative {
        pos -= 1;
        buf[pos] = b'-';
    }

    core::str::from_utf8(&buf[pos..]).unwrap_or("?")
}

/// 格式化进度百分比
pub fn format_progress(pct: u8, buf: &mut [u8; 8]) -> &str {
    let mut pos = 0;

    // 数字部分
    if pct >= 100 {
        buf[pos] = b'1';
        pos += 1;
        buf[pos] = b'0';
        pos += 1;
        buf[pos] = b'0';
        pos += 1;
    } else if pct >= 10 {
        buf[pos] = b'0' + pct / 10;
        pos += 1;
        buf[pos] = b'0' + pct % 10;
        pos += 1;
    } else {
        buf[pos] = b'0' + pct;
        pos += 1;
    }

    buf[pos] = b'%';
    pos += 1;

    core::str::from_utf8(&buf[..pos]).unwrap_or("?%")
}
