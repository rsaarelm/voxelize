view model front back:
    # Make the initial frame so we can start a viewer
    cargo run --example spritedump -- {{model}} {{front}} {{back}}
    sxiv output.png &
    # Monitor the model for changes and keep regeneratting the image
    echo {{model}} | entr -s 'cargo run --example spritedump -- {{model}} {{front}} {{back}}'
