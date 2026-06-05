# Darwincode Project Configuration

This directory contains configuration files for `darwincode` that are specific to this project repository.

## Custom Themes
You can add custom theme JSON files under the `themes/` directory.
For example, a theme file named `themes/my_cool_theme.json` will be discovered automatically and can be activated by running `/theme` or setting `"theme": "Custom(my_cool_theme)"` in your config.

To get started, check out the template theme at `themes/custom_template.json`.

All theme config structures must conform to the schema:
https://raw.githubusercontent.com/femboypig/darwincode/main/theme.json
