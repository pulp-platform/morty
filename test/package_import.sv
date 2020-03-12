`include "include.svh"

import global_import_pkg0::*;
import global_import_pkg1::heho;

module bla;
endmodule

module blib import head0_import_pkg::*; import head1_import_pkg::*; (
    input clk_i,
    input blib_pkg::lala in_i
);

    localparam param_import_pkg::T Test = 1;

    always_comb begin
        in_i = head0_pkg::CONST;
    end
endmodule

`define IMPORT(__pkg)  \
    import __pkg::*;

module lala;
    // pragma translate_off
    // Test run
    `IMPORT(body2_import_pkg) // this is a macro
    always_comb begin
        a <= b;
    end
    // pragma translate_on
endmodule