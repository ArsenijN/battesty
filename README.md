# battesty
![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)

Replacement for Windows's ETA of work from battery.


## Why it specifically exists?
By some reason, Windows 11 doesn't like being on my HP EliteBook 830 G5 w/Intel i5 8350U, and doesn't like to show the remaining time of work from battery, **but** my friend's HP EliteBook 845 G10 w/Ryzen 5 7545U have not only "ETA", but also new battery icon!!!!!!!!! (im angy)

This is why I want to replace the Windows's battery icon with mine, and also "implement" (inspire from; add simmilar features from) AccuBattery to the battesty (why not?)


## Current state of the app
It's currently not meant to be used by anyone, not a candidate for release for soon, use it for your own risk and change the settings only if you know what they do. Documentation pending.

## ToDo
- Fix background of icon in taskbar (currently it's white)
- Fix lightning bolt color (currently it's black(!))
- Change icon canvas size to match Hi-DPI better and improve visivility and visual details
- Update the icon on app start, not on time to measure (so it will not hang on latest saved measurement)
- (older task) calculate annual capacity loss and determined battery capacity (not necessary since batteries can tell by themself what's their full capacity and design one). Aka I can't remember if 2.5% annual loss isn't hardcoded, and I also think that I should use API to get battery discharge current (and charge current), and also some handy info from the battery, not only depending on the percentage of the battery
- Optimize for SSD wear (skip if Windows do that by default with any changing files)


that's mainly all I think. Any support will be appreciated
