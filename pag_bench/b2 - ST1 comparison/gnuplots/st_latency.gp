set terminal pdf enhanced color font "Helvetica,14" size 6,8 

set style line 1 lc rgb "#0060ad" lt 1 lw 1 pt 2 ps 1
set style line 2 lc rgb "#0060ad" lt 1 lw 1 pt 2 ps 1 dashtype "_"
set style line 3 lc rgb "#00796B" lt 1 lw 1 pt 2 ps 1
set style line 4 lc rgb "#00796B" lt 1 lw 1 pt 2 ps 1 dashtype "_"
set style line 5 lc rgb "#8BC34A" lt 1 lw 1 pt 2 ps 1
set style line 6 lc rgb "#8BC34A" lt 1 lw 1 pt 2 ps 1 dashtype "_"
set style line 7 lc rgb "#F4511E" lt 1 lw 1 pt 2 ps 1
set style line 8 lc rgb "#F4511E" lt 1 lw 1 pt 2 ps 1 dashtype "_"
set style line 9 lc rgb "#F88967" lt 1 lw 1 pt 2 ps 1
set style line 10 lc rgb "#F88967" lt 1 lw 1 pt 2 ps 1 dashtype "_"
set style line 11 lc rgb "#BC3409" lt 1 lw 1 pt 2 ps 1
set style line 12 lc rgb "#BC3409" lt 1 lw 1 pt 2 ps 1 dashtype "_"
set style line 13 lc rgb "#BC08A7" lt 1 lw 1 pt 2 ps 1 dashtype "."

set logscale y 10
set format y "10^{%L}"
set yrange [0.001:2]

set samples 20000 

set output "plots/st_latency.pdf"

set multiplot layout 4,1 rowsfirst
set xlabel "epoch"
set ylabel "t [s]"

set title "PAG construction offline latency [150K]"
plot \
  "prepped/prepped_st_1_5.csv" using 1:2 with lines ls 1 smooth bezier title "w1", \
  "prepped/prepped_st_2_5.csv" using 1:2 with lines ls 3 smooth bezier title "w2", \
  "prepped/prepped_st_4_5.csv" using 1:2 with lines ls 5 smooth bezier title "w4", \
  "prepped/prepped_st_8_5.csv" using 1:2 with lines ls 7 smooth bezier title "w8", \
  "prepped/prepped_st_16_5.csv" using 1:2 with lines ls 9 smooth bezier title "w16", \
  "prepped/prepped_tc_32_5.csv" using 1:2 with lines ls 13 smooth bezier title "tri32", \

set title "PAG construction offline latency [250K]"
plot \
  "prepped/prepped_st_1_50.csv" using 1:2 with lines ls 1 smooth bezier title "w1", \
  "prepped/prepped_st_2_50.csv" using 1:2 with lines ls 3 smooth bezier title "w2", \
  "prepped/prepped_st_4_50.csv" using 1:2 with lines ls 5 smooth bezier title "w4", \
  "prepped/prepped_st_8_50.csv" using 1:2 with lines ls 7 smooth bezier title "w8", \
  "prepped/prepped_st_16_50.csv" using 1:2 with lines ls 9 smooth bezier title "w16", \
  "prepped/prepped_tc_32_50.csv" using 1:2 with lines ls 13 smooth bezier title "tri32", \

set title "PAG construction offline latency [500K]"
plot \
  "prepped/prepped_st_1_200.csv" using 1:2 with lines ls 1 smooth bezier title "w1", \
  "prepped/prepped_st_2_200.csv" using 1:2 with lines ls 3 smooth bezier title "w2", \
  "prepped/prepped_st_4_200.csv" using 1:2 with lines ls 5 smooth bezier title "w4", \
  "prepped/prepped_st_8_200.csv" using 1:2 with lines ls 7 smooth bezier title "w8", \
  "prepped/prepped_st_16_200.csv" using 1:2 with lines ls 9 smooth bezier title "w16", \
  "prepped/prepped_tc_32_200.csv" using 1:2 with lines ls 13 smooth bezier title "tri32", \

