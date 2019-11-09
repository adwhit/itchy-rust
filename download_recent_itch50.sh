set -e

# find the smallest ITCH50.gz available on the ftp server today and print its name

## curl for a list of files, find ITCH50 files, sort them by size (3rd column)
curl -k --verbose ftp://emi.nasdaq.com/ITCH/
# MD_FILENAME="$(curl -s ftp://emi.nasdaq.com/ITCH/ | egrep "PSX_ITCH_?50\.gz$" | sort -k3 -h | head -1 | awk ' { print $4 } ')"
# echo "$MD_FILENAME"

# Use the name to download it
# curl -v "ftp://emi.nasdaq.com/ITCH/${MD_FILENAME}" -o ${MD_FILENAME}

# download its checksum.
# curl -v "ftp://emi.nasdaq.com/ITCH/${MD_FILENAME}.md5sum" -o "${MD_FILENAME}.md5sum"

# if the checksum matches the expected checksum
# if diff <(md5sum ${MD_FILENAME} | awk ' { print $1 } ') <(cat "${MD_FILENAME}.md5sum" | awk ' { print $1 } ') ; then
#     # if all passes, set the environment variable to file name, so cargo test can use it
#     gunzip ${MD_FILENAME}
#     # since we have g-unzipped the file, we need to remove ".gz" from the filename
#     export MARKET_DATA_TEST_FILE=${MD_FILENAME%".gz"}
# fi
