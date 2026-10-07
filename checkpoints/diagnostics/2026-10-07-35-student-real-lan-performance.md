# Hai lượt Windows Teacher → 35 Windows Student trong phòng thật — 07-10-2026

## Phạm vi và nguồn

Đọc log Teacher cục bộ và source; không phát phiên chiếu, chạy test, build, restart hay deploy trong lần phân tích này. Chủ phòng bổ sung: **lượt 1 mở video YouTube trên Teacher; lượt 2 không mở video, chỉ thao tác cơ bản**. Hai lượt có workload khác nhau, không phải A/B cùng nội dung. Chủ phòng cũng thông báo kỳ thi đã kết thúc và cho phép deploy Student mới lên các máy khi có build tiếp theo.

- Teacher `10.0.55.10`, i7-14700 / 28 logical CPUs, Windows 10 Education; Ethernet Intel I219-LM đang Up / 1 Gbps qua truy vấn adapter cục bộ.
- Cả hai stream có 35 receiver thực tế, một encoder Intel Quick Sync H.264, bốn worker fan-out, capture/encode 1920×1080. Nếu tính Teacher thì tổng cộng 36 máy; không gọi đây là 36 Student hay nghiệm thu 40 Student.
- Teacher build `1791347790-teacher-adaptive-20261007`; Student vẫn là build cũ `1791341267-paired-20261007-094727`.
- Nguồn: `%APPDATA%/com.classroom/classroom_console/teacher-log.txt`, `ui.log.txt`. Snapshot theo stream và số liệu từng máy tại `target/large-session-analysis-20261007/`; đây là artifact cục bộ có thể bị dọn khi clean target.
- Lượt 1 stream `8994680672841960415`, UI 11:47:45.329–11:49:20.656 (+07), media 94.925 s.
- Lượt 2 stream `8994680672841960416`, UI 11:49:25.523–11:50:03.118 (+07), media 37.449 s.

## Kết quả

| Chỉ số | Lượt 1 | Lượt 2 |
|---|---:|---:|
| Receiver | 35 | 35 |
| Độ phân giải | 1920×1080 | 1920×1080 |
| Encode trung bình | 28.68 FPS | 26.94 FPS |
| Encode p50 / p95 | 9.01 / 13.17 ms | 10.17 / 12.98 ms |
| Decode từng Student, trung bình từ counter | 17.91–18.19 FPS | 25.87–26.57 FPS |
| Render từng Student, trung bình từ counter | 16.47–17.33 FPS | 25.38–25.82 FPS |
| Khoảng report không tăng counter decode / máy | 29–31 | 0 |
| Khoảng liên tục dài nhất không tăng counter decode / máy | 15.015–16.090 s | 0 |
| CPU Teacher median, chuẩn hóa theo toàn host | 3.24% | 3.62% |
| CPU Teacher max theo interval | 6.17% | 5.99% |
| Working set median / max | 155.85 / 159.21 MiB | 153.77 / 168.08 MiB |
| Bitrate encoded trung bình | 5.67 Mbps | 5.33 Mbps |
| Aggregate send trung bình / max interval | 173.70 / 430.04 Mbps | 192.25 / 437.94 Mbps |
| Packet gửi thành công | 2,012,549 | 856,710 |
| Recipient-datagram bị bỏ khi gửi AU lỗi | 316,730 | 5,795 |
| Bỏ / (gửi + bỏ) | 13.60% | 0.67% |
| Send errors / WouldBlock / retry recovered | 18,156 / 19,002 / 846 | 245 / 714 / 469 |
| Send retry-budget / deadline exhausted | 15,792 / 259 | 123 / 100 |
| NACK requested sequences | 58,198 | 5,411 |
| NACK cache misses | 56,623 (97.29%) | 0 |
| Repair packets sent / send errors | 1,351 / 224 | 4,323 / 1,088 |
| Repair WouldBlock / retry-budget exhausted | 306 / 205 | 1,490 / 1,027 |
| Missing / received theo counter Student | 1.901–3.485% | 0.270–0.931% |
| RTT median từng máy | 0.365–0.978 ms | 0.313–0.839 ms |
| RTT max từng máy | 679.959–723.558 ms | 29.329–77.656 ms |
| Keyframe encoded | 41 | 13 |
| AU lớn nhất | 361,003 bytes | 310,658 bytes |
| Target bitrate đầu → cuối | 6 → 1.5 Mbps | 6 → 6 Mbps |

Counter gửi thành công chỉ chứng minh socket chấp nhận datagram, không chứng minh receiver nhận/render. Abandoned là datagram theo từng recipient, có thể gồm phần AU không thử gửi sau lỗi đầu tiên; không cộng thêm send errors vào tỷ lệ này. Missing/received là tỷ lệ counter hiện tại, bao gồm xử lý late/expired/reordered, không phải phép đo độc lập packet loss vật lý của LAN. FPS Student dùng chênh lệch counter giữa report đầu/cuối; cửa sổ khác toàn stream Teacher. CPU/RAM là process media helper đang ghi metrics, không phải tổng toàn máy hoặc tổng mọi GUI/helper.

## Điều đã chứng minh

1. **Encoder hoạt động tốt ở môi trường thật này.** Chỉ một instance, p95 ~13 ms, CPU host-capacity ~3–6%. Không có adaptive CPU/encode pressure. Bản sửa chuẩn hóa CPU không còn tự hạ chất lượng do đọc nhầm summed-core CPU ~100–148% thành quá tải host.
2. **Lượt 1 không đạt chất lượng chiếu mượt.** Counter decode đồng loạt ngừng tăng trên 35 máy trong nhiều khoảng, khoảng dài nhất 15–16 giây. Các nhóm report toàn lớp đứng ở khoảng giây 29–36, 39–44 và 78–91 tính từ yêu cầu start. Packet receive vẫn tăng và Teacher vẫn encode quanh 30 FPS, nên không giải thích được bằng một encoder ngừng chạy hay một Student riêng lẻ chậm.
3. **Đường gửi có lỗi thực sự, tương quan với các đoạn đứng.** Tại khoảng giây 28.9 có delta ~1,047 send errors và 11,450 abandoned; nhiều interval lỗi ~1,050, gần 35 × 30 AU. Log có Windows `10035` / WouldBlock ở video và pointer. Retry-budget exhausted áp đảo deadline exhausted: chỉ kéo dài deadline không giải quyết hết.
4. **Repair lượt 1 gần như không cứu được stream.** 97.29% sequence được yêu cầu không còn trong cache. Lượt 2 không cache miss nhưng 1,088 / 5,411 yêu cầu gặp repair send error (~20.1%); không thể gọi repair đã tốt chỉ vì cache hit.
5. **Thao tác cơ bản tốt hơn video rõ rệt; tải video chưa đạt.** Lượt 2 không có interval decode đứng theo report, render ~25–26 FPS. Nó ngắn hơn, không chạy YouTube và có hoạt động màn hình thấp hơn: dirty ratio 0.213 so với 0.423; capture 17.60 so với 31.81 FPS và có repeated submissions. Lượt 1 có video chuyển động liên tục nhưng encoder vẫn tốt, trong khi sender/recovery suy giảm. Không quy chênh lệch thành hiệu quả của một thay đổi code/cấu hình giữa hai lượt; cũng chưa có đối chứng cùng video với cùng thời lượng.

## Nguyên nhân được source hỗ trợ và phần còn là giả thuyết

`fanout.rs` chia tốc độ pacing cho bốn worker, nhưng `PacketPacer` trong `udp_sender.rs` lấy thời gian lý tưởng rồi **clamp về max span**, hiện tối đa 8 ms (hoặc 1/3 frame period). Với AU 361,003 bytes và 35 receiver, payload phải gửi là 12.635 MB: riêng serialize ở 1 Gbps đã cần **101.1 ms**, chưa tính overhead. 400 Mbps cấu hình cần ~252.7 ms. Clamp 8 ms làm giới hạn tốc độ danh nghĩa không còn được giữ cho AU lớn. Gửi syscall thực tế có thể lâu hơn 8 ms; 8 ms là pacing target, không phải số đo thời gian ra dây.

Đây là bất tương thích ngân sách burst có thể xác định từ code và dữ liệu, rất phù hợp với WouldBlock chung lớp. Chưa có trace kernel/NIC để quy mọi lỗi cho duy nhất một nguyên nhân. Average network <1 Gbps không loại trừ quá tải tức thời.

Repair/pointer chưa chung aggregate governor với video. Repair cache TTL 300 ms / tối đa 4096 packet; cache miss hiện không phân loại expired, capacity eviction, chưa cache hay sequence invalid. NACK service interval p95 tối đa chỉ ~2.02 ms, trong khi Teacher control-dispatch p95 có interval ~396 ms ở lượt 1. NACK được router xử lý riêng trước forward: không được kết luận 396 ms ấy chính là độ trễ NACK hoặc nguyên nhân hết TTL. Cần đo tuổi packet và thời điểm phát/nhận NACK.

Student tiếp tục nhận packet nhưng không decode phù hợp với mất reference H.264 và phục hồi không thành công. Không có decoder-local logs trong lượt phân tích này để chứng minh chính xác trạng thái. Keyframe gate có window tăng tới 1,600 ms khi storm; nó có thể kéo dài recovery nhưng tự nó không đủ giải thích đứng 15–16 giây. Keyframe lớn có thể tiếp tục bị mất trên cùng đường gửi.

Mean bitrate được cập nhật qua CodecAPI; còn có refinement bitrate/QP và keyframe. Lượt 1 target giảm 8 lần xuống 1.5 Mbps (7 receiver_degradation, 1 receiver_recovery), nhưng encoded trung bình toàn lượt 5.67 Mbps. Không lấy hai cửa sổ khác nhau này để kết luận MFT bỏ qua bitrate; cần log applied base/refinement bitrate và byte output sau mỗi lần đổi.

## Thứ tự sửa/cải tiến

### P0 — Teacher: tốc độ gửi và ngân sách AU

- Thay clamp pacing bằng giới hạn tốc độ aggregate thật cho video + repair + pointer; giữ burst nhỏ và không nhân burst theo worker. Đo tổng wire budget theo số recipient và overhead.
- Trước khi gửi AU, so nhu cầu byte × recipients với freshness budget; nếu không thể gửi kịp, giảm tải encoder/bitrate/FPS và quản lý drop theo reference/keyframe. Không giải quyết bằng hàng đợi dài, gửi AU cũ vô hạn, tăng socket buffer vô hạn hay retry lâu trên 35 endpoint.
- Keyframe/refinement phải tuân theo cùng ngân sách. Cần tránh vòng loss → IDR lớn → burst → loss; cân nhắc bounded thời gian hoàn thành recovery AU và chính sách giữ/bỏ AU đang gửi được đo đối chứng.
- Log deadline toàn AU, actual pacing rate/wait, enqueue→send-start→last-datagram, bytes/drop theo AU/keyframe và recipients. Capture→handoff hiện không phải latency cuối mạng.

### P0 — Student + Teacher: phục hồi reference nhanh và có giới hạn

- Student bỏ AU hỏng/đã quá hạn, bỏ packet cũ và NACK quá hạn, quản lý rõ chờ IDR. Phát yêu cầu recovery kịp thời, coalesce nhưng không mất yêu cầu đang chờ; IDR tới phải đưa decode/render trở lại nhanh.
- Teacher phân loại cache miss và đo NACK age; chặn retry/repair stale, dành ngân sách gửi có giới hạn cho repair/recovery. Không nới jitter/TTL thành vài giây để che vấn đề.
- Tách chỉ số ingress, depacketize, waiting-IDR, decode và render để phân biệt lỗi network/reference với decoder chậm. Giữ protocol compatibility hoặc nâng version rõ nếu phải thay wire.

### P1 — Adaptive chất lượng và render

- Adaptive dựa trên bitrate output thực, fanout size và khả năng gửi; refinement không nâng burst khi receiver đang mất gói/đứng. Giữ 1080p, ưu tiên freshness và text; giảm FPS/bitrate có kiểm soát trước giảm resolution.
- Sau khi transport/recovery ổn, tối ưu khoảng decode→render. Lượt 2 chênh lệch nhỏ ~0.05–1.19 FPS tùy máy, không có bằng chứng renderer là nút thắt chung chính. Một số máy có render-call p95 gần 30 ms: cần Student-local logs/diagnostics để phân biệt vsync wait và xử lý tốn CPU.

### Đo lại sau bản sửa

Chủ phòng đã cho phép cập nhật Student sau kỳ thi. Build Teacher + Student cùng source và bundle ZIP mới theo PROJECT_NOTES, deploy bằng updater Classroom, giữ identity/settings/logs; xác minh từng máy đã báo build mới. Không dùng SSH/SCP đè binary đang chạy.

Lặp workload có mốc thời gian cố định (text, cuộn trang, motion), giữ 1080p; đo ở 1/5/10/20/35/40 endpoint thực, thêm lượt dài và latency quang học. Không tự coi short-run hoặc FPS counter là chứng minh độ trễ đầu-cuối. Hai lượt này là bằng chứng thực tế 1080p/35 receiver, **chưa đạt nghiệm thu chiếu mượt 40 receiver**.

## Bảng từng Student

Các giá trị sau được chép từ analysis.json; FPS là counter-derived, missing% theo định nghĩa nêu trên. `Zero` là số interval report counter decode không tăng, không phải số lần popup/lỗi decoder.

### L??t 1

| ID | M?y | Decode FPS | Render FPS | Missing% | Zero | RTT median / max ms |
|---:|---|---:|---:|---:|---:|---:|
| 1 | 505-11 | 18.12 | 17.33 | 2.305 | 30 | 0.956 / 708.336 |
| 2 | 505-01 | 18.17 | 16.92 | 2.653 | 29 | 0.365 / 691.341 |
| 3 | 505-24 | 18.17 | 16.88 | 2.468 | 29 | 0.446 / 698.743 |
| 4 | 505-03 | 18.12 | 16.80 | 1.967 | 30 | 0.390 / 679.959 |
| 5 | 505-20 | 18.16 | 16.84 | 1.901 | 30 | 0.454 / 706.752 |
| 6 | 505-32 | 18.18 | 16.81 | 2.409 | 30 | 0.520 / 706.156 |
| 7 | 505-14 | 18.17 | 16.93 | 3.451 | 29 | 0.770 / 717.896 |
| 8 | 505-08 | 18.12 | 16.81 | 2.463 | 29 | 0.374 / 707.912 |
| 9 | 505-04 | 18.06 | 16.69 | 2.091 | 30 | 0.450 / 709.700 |
| 10 | 505-23 | 18.17 | 16.97 | 2.357 | 29 | 0.432 / 709.796 |
| 11 | 505-18 | 18.17 | 16.96 | 3.485 | 29 | 0.443 / 723.558 |
| 12 | 505-02 | 18.12 | 16.82 | 2.614 | 29 | 0.396 / 711.165 |
| 13 | 505-19 | 18.15 | 16.86 | 2.260 | 30 | 0.444 / 685.770 |
| 14 | 505-13 | 18.17 | 16.87 | 3.065 | 29 | 0.387 / 691.004 |
| 15 | 505-09 | 18.11 | 16.61 | 2.879 | 29 | 0.496 / 708.288 |
| 16 | 505-27 | 18.16 | 16.89 | 2.599 | 29 | 0.406 / 723.274 |
| 17 | 505-22 | 18.02 | 16.73 | 2.383 | 30 | 0.467 / 692.347 |
| 18 | 505-15 | 18.15 | 17.03 | 2.692 | 29 | 0.734 / 706.051 |
| 19 | 505-21 | 18.17 | 16.85 | 3.458 | 29 | 0.413 / 696.092 |
| 20 | 505-16 | 18.19 | 16.85 | 2.430 | 29 | 0.754 / 704.560 |
| 21 | 505-10 | 17.91 | 16.47 | 2.030 | 31 | 0.476 / 706.918 |
| 23 | 505-36 | 18.10 | 16.60 | 2.380 | 29 | 0.488 / 707.724 |
| 24 | 505-17 | 18.16 | 16.95 | 2.996 | 29 | 0.438 / 704.059 |
| 25 | 505-12 | 18.03 | 16.62 | 2.291 | 30 | 0.484 / 703.435 |
| 26 | 505-34 | 18.18 | 16.82 | 2.845 | 29 | 0.978 / 719.714 |
| 27 | 505-07 | 18.01 | 16.70 | 2.973 | 29 | 0.484 / 715.372 |
| 28 | 505-05 | 18.16 | 16.81 | 2.575 | 29 | 0.514 / 713.002 |
| 29 | 505-26 | 17.98 | 16.70 | 2.442 | 31 | 0.567 / 722.167 |
| 30 | 505-30 | 18.14 | 16.79 | 2.407 | 29 | 0.540 / 722.520 |
| 31 | 505-28 | 18.19 | 16.79 | 3.022 | 29 | 0.508 / 713.105 |
| 32 | 505-33 | 18.18 | 16.86 | 2.823 | 30 | 0.937 / 706.956 |
| 33 | 505-31 | 18.15 | 16.74 | 2.415 | 30 | 0.536 / 711.374 |
| 34 | 505-29 | 18.10 | 16.74 | 2.808 | 29 | 0.510 / 706.536 |
| 35 | 505-06 | 18.09 | 16.76 | 3.260 | 30 | 0.466 / 701.643 |
| 36 | 505-35 | 17.96 | 16.52 | 2.316 | 30 | 0.575 / 687.951 |

### L??t 2

| ID | M?y | Decode FPS | Render FPS | Missing% | Zero | RTT median / max ms |
|---:|---|---:|---:|---:|---:|---:|
| 1 | 505-11 | 26.28 | 25.64 | 0.856 | 0 | 0.835 / 50.074 |
| 2 | 505-01 | 26.42 | 25.67 | 0.558 | 0 | 0.313 / 37.060 |
| 3 | 505-24 | 26.40 | 25.65 | 0.713 | 0 | 0.386 / 48.149 |
| 4 | 505-03 | 26.42 | 25.81 | 0.597 | 0 | 0.343 / 37.680 |
| 5 | 505-20 | 26.40 | 25.68 | 0.746 | 0 | 0.436 / 49.784 |
| 6 | 505-32 | 26.29 | 25.71 | 0.825 | 0 | 0.496 / 43.601 |
| 7 | 505-14 | 25.87 | 25.43 | 0.676 | 0 | 0.711 / 33.403 |
| 8 | 505-08 | 26.23 | 25.76 | 0.817 | 0 | 0.396 / 29.329 |
| 9 | 505-04 | 26.07 | 25.46 | 0.445 | 0 | 0.390 / 48.248 |
| 10 | 505-23 | 26.26 | 25.65 | 0.741 | 0 | 0.387 / 44.581 |
| 11 | 505-18 | 26.26 | 25.62 | 0.848 | 0 | 0.368 / 41.808 |
| 12 | 505-02 | 26.26 | 25.73 | 0.931 | 0 | 0.366 / 49.705 |
| 13 | 505-19 | 26.42 | 25.70 | 0.876 | 0 | 0.400 / 47.873 |
| 14 | 505-13 | 26.26 | 25.59 | 0.746 | 0 | 0.320 / 37.125 |
| 15 | 505-09 | 25.91 | 25.41 | 0.674 | 0 | 0.438 / 43.861 |
| 16 | 505-27 | 26.40 | 25.79 | 0.601 | 0 | 0.405 / 45.172 |
| 17 | 505-22 | 26.26 | 25.62 | 0.929 | 0 | 0.415 / 44.812 |
| 18 | 505-15 | 26.26 | 25.62 | 0.664 | 0 | 0.736 / 32.196 |
| 19 | 505-21 | 26.29 | 25.76 | 0.790 | 0 | 0.401 / 44.852 |
| 20 | 505-16 | 26.23 | 25.74 | 0.716 | 0 | 0.703 / 39.718 |
| 21 | 505-10 | 26.21 | 25.57 | 0.323 | 0 | 0.458 / 74.836 |
| 23 | 505-36 | 26.27 | 25.66 | 0.679 | 0 | 0.424 / 43.006 |
| 24 | 505-17 | 26.40 | 25.76 | 0.805 | 0 | 0.398 / 40.268 |
| 25 | 505-12 | 26.43 | 25.74 | 0.732 | 0 | 0.408 / 42.007 |
| 26 | 505-34 | 26.27 | 25.71 | 0.594 | 0 | 0.806 / 38.427 |
| 27 | 505-07 | 26.07 | 25.41 | 0.270 | 0 | 0.422 / 74.662 |
| 28 | 505-05 | 26.40 | 25.74 | 0.612 | 0 | 0.374 / 43.126 |
| 29 | 505-26 | 25.90 | 25.38 | 0.455 | 0 | 0.443 / 74.323 |
| 30 | 505-30 | 26.07 | 25.49 | 0.331 | 0 | 0.457 / 75.741 |
| 31 | 505-28 | 26.27 | 25.66 | 0.825 | 0 | 0.443 / 40.521 |
| 32 | 505-33 | 26.57 | 25.76 | 0.270 | 0 | 0.839 / 77.656 |
| 33 | 505-31 | 26.57 | 25.82 | 0.527 | 0 | 0.494 / 36.338 |
| 34 | 505-29 | 26.09 | 25.51 | 0.352 | 0 | 0.441 / 70.044 |
| 35 | 505-06 | 26.27 | 25.71 | 0.700 | 0 | 0.427 / 39.303 |
| 36 | 505-35 | 26.43 | 25.80 | 0.750 | 0 | 0.465 / 52.555 |
