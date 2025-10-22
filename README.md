# rusty-pedalbox

This project is an experiment to create a USB pedalbox with an STM32F407.

## Components

- STM32F407G-DISC1 board

Generated from [Embassy STM32F4 Template](https://github.com/Krizsi96/embassy-stm32f4discovery-template) using [`cargo generate`](https://github.com/cargo-generate/cargo-generate).

## How to generate the HID report?

- The `hidrd.xsd` contains the xml schema for the `.xml` file
- The `pedalbox_hid.xml` contains the definition of the HID descriptor for the pedalbox.

1. Modify the `pedalbox_hid.xml` file according to the schema to define your device.
2. Run the `hidrd-convert` tool to generate C source code output:
```shell
$ hidrd-convert -i xml -o code pedalbox_hid.xml
0x05, 0x01,                     /*  Usage Page (Desktop),           */
0x09, 0x08,                     /*  Usage (Multi Axis Ctrl),        */
0xA1, 0x01,                     /*  Collection (Application),       */
0x09, 0x30,                     /*      Usage (X),                  */
0x14,                           /*      Logical Minimum (0),        */
0x27, 0xFF, 0xFF, 0x00, 0x00,   /*      Logical Maximum (65535),    */
0x75, 0x10,                     /*      Report Size (16),           */
0x95, 0x01,                     /*      Report Count (1),           */
0x81, 0x02,                     /*      Input (Variable),           */
0x09, 0x31,                     /*      Usage (Y),                  */
0x14,                           /*      Logical Minimum (0),        */
0x27, 0xFF, 0xFF, 0x00, 0x00,   /*      Logical Maximum (65535),    */
0x75, 0x10,                     /*      Report Size (16),           */
0x95, 0x01,                     /*      Report Count (1),           */
0x81, 0x02,                     /*      Input (Variable),           */
0xC0                            /*  End Collection                  */
```
3. Copy the output values to the `PEDALBOX_REPORT_DESCRIPTOR` in the `usb.rs` file.
4. If needed then change the `PedalboxReport` structure.
hae;lajf\
