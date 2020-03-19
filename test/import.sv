`define IMPORT(__pkg)  \
    /* hallo */        \
    import __pkg::*;

module lala;
    // pragma translate_off
    // Test run
    `IMPORT(body2_import_pkg) // asd

    // sd
    always_comb begin
        a <= b;
    end
    // pragma translate_on
endmodule