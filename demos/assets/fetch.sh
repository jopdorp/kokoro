#!/bin/sh
# Fetches the converted BoomBox (2048 px) and FlightHelmet (1024 px), block-linear with full mips, from the fork's assets branch.
set -e
fetch() {
    dir=$(dirname "$0")/$1
    [ -f "$dir/manifest.json" ] && return
    mkdir -p "$dir"
    curl -fL https://raw.githubusercontent.com/jopdorp/kokoromi/f00cf49e1e03cc2f3728f73ea0c09a81a27ddb1f/$1.tar.gz -o "$dir/$1.tar.gz"
    echo "$2  $dir/$1.tar.gz" | sha256sum -c -
    tar -xzf "$dir/$1.tar.gz" -C "$dir" && rm "$dir/$1.tar.gz"
}
fetch boombox 8256f5d473da5d0df8a7f36c9d85f6a1eeaeaa43b8e6ae99bc0aa4e9579c55c7
fetch flighthelmet 9c409e8ceeb8e5f8500f4d901f4dbb4abb5026ca464f589dbc584f2965dfa1b4

fetch_lofi() {
    dir=$(dirname "$0")/boombox-lofi
    [ -f "$dir/lofi.pcm" ] && return
    mkdir -p "$dir"
    curl -fL https://raw.githubusercontent.com/jopdorp/kokoromi/87945844df4c0740733db4c6c26fabfed954dfd6/boombox-lofi.tar.xz -o "$dir/lofi.tar.xz"
    echo "2f053d8b20cfb3f7222bd8625f5e5b9b86bfd21fad48c6fe64d06e99df32c876  $dir/lofi.tar.xz" | sha256sum -c -
    tar -xJf "$dir/lofi.tar.xz" -C "$dir" && rm "$dir/lofi.tar.xz"
}
fetch_lofi

fetch_dungeon() {
    dir=$(dirname "$0")/dungeon
    [ -f "$dir/manifest.json" ] && return
    mkdir -p "$dir"
    curl -fL https://raw.githubusercontent.com/jopdorp/kokoromi/e6cb14cfdd2fe60ca233fd5e1eb9d99f42e6b846/dungeon-materials.tar.xz -o "$dir/materials.tar.xz"
    echo "de139bb5257dc508b5f05308511ece3d14717529b101dbe36fc30d9db34bef4e  $dir/materials.tar.xz" | sha256sum -c -
    tar -xJf "$dir/materials.tar.xz" -C "$dir" && rm "$dir/materials.tar.xz"
}
fetch_dungeon

fetch_dungeon_audio() {
    dir=$(dirname "$0")/dungeon-audio
    [ -f "$dir/SOURCES" ] && return
    mkdir -p "$dir"
    curl -fL https://raw.githubusercontent.com/jopdorp/kokoromi/f42255ba079ee4bf82f4686957f9b175c932a7a2/dungeon-audio.tar.xz -o "$dir/audio.tar.xz"
    echo "ac6b6157c7a2f8016da6b3434d80e653f47345a9e86c4884a0e78a7e041a688d  $dir/audio.tar.xz" | sha256sum -c -
    tar -xJf "$dir/audio.tar.xz" -C "$dir" && rm "$dir/audio.tar.xz"
}
fetch_dungeon_audio

fetch_hrir() {
    dir=$(dirname "$0")/hrir
    [ -f "$dir/d1.bin" ] && return
    mkdir -p "$dir"
    curl -fL https://raw.githubusercontent.com/jopdorp/kokoromi/c0f3f7d917634f085cef1c41da0880e56ddb716d/sadie-d1-hrir.tar.xz -o "$dir/hrir.tar.xz"
    echo "258a8b442cc0589cc6d069654c5535a8eb5926f2d3620b13ae4586f2961ca1ec  $dir/hrir.tar.xz" | sha256sum -c -
    tar -xJf "$dir/hrir.tar.xz" -C "$(dirname "$dir")" && rm "$dir/hrir.tar.xz"
}
fetch_hrir
