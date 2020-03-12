
module test import blub_pkg::*; (
    input blub_pkg::T clk_i
);
    /* Testonen */
    localparam blub_pkg::A C = 20;
    logic blub_pkg = blub_pkg::C;
endmodule

module test2;
    test i_test ();
endmodule

