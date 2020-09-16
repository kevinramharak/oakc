#/usr/bin/bash

# primitive, has to be run from project root

for file in ./examples/*.ok
do
    output=${file/examples/mar}
    output=${output/.ok/.mar}
    cargo run -- --mar c $file && mv main.mar $output
done
