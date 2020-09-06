#!/bin/bash

sudo nice --15 sudo -u $USER cargo bench insertion
sudo nice --15 sudo -u $USER cargo bench lookup

write_csv() {
    find "$1" -type d -name new | while read l; do
        MAP_NAME=$(jq '.function_id' "$l/benchmark.json" | cut -d'(' -f1 | sed -e 's/^"//; s/ *$//; s/ /_/g')
        ELEMS=$(jq '.throughput.Elements' "$l/benchmark.json")
        MEAN=$(jq '.mean.point_estimate' "$l/estimates.json")
        echo "$ELEMS $MAP_NAME $MEAN"
    done | sort -n | ghc -e 'do
        ls <- map words . lines <$> getContents
        let sizes = groupBy ((==) `on` head) ls
        putStr "Size "
        putStrLn $ unwords $ map (\[_size, name, _time] -> name) $ head sizes
        forM_ sizes $ \sz -> do
            let size = read $ head (head sz) :: Double
            let times = map (\[_size, _name, time] -> read time) sz :: [Double]
            -- time in ns
            -- rate in Melems / second
            let rates = map (\time -> 1000 * size / time) times
            putStr $ show size ++ " "
            putStrLn $ unwords $ map show rates
    ' | sed -e 's/ /,/g; s/_/\\_/g'
}

write_csv target/criterion/insertion > insertion.csv
write_csv target/criterion/lookup > lookup.csv

gnuplot <<EOF
reset
set datafile separator ','
set key autotitle columnhead noenhanced
set logscale x
set style line 12 lc rgb '#808080' lt 0 lw 1
set grid back ls 12
set title 'Insertion rate (bigger is better)'
set xlabel 'Number of elements'
set ylabel 'Insertion rate (Melems/second)'
set terminal svg size 700,524 font 'Verdana,16'
set output 'insertion.svg'
plot for [i=2:7] 'insertion.csv' using 1:i with linespoints pt 6 lw 2
EOF

gnuplot <<EOF
reset
set datafile separator ','
set key autotitle columnhead noenhanced
set logscale x
set style line 12 lc rgb '#808080' lt 0 lw 1
set grid back ls 12
set title 'Lookup rate (bigger is better)'
set xlabel 'Number of elements'
set ylabel 'Lookup rate (Melems/second)'
set terminal svg size 700,524 font 'Verdana,16'
set output 'lookup.svg'
plot for [i=2:7] 'lookup.csv' using 1:i with linespoints pt 6 lw 2
EOF

rm {insertion,lookup}.csv
