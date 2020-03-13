module test import blub_pkg::*; #(
    parameter blub_pkg::T my_param = blub_pkg::constant
) (
    input blub_pkg::T clk_i
);
    /* Testonen */
    localparam blub_pkg::A C = 20;
    logic blub_pkg = blub_pkg::C;

    always_comb begin : prepare_input
        for (int unsigned i = 0; i < NUM_OPERANDS; i++) begin
        local_operands[i] = operands_i[i] >> LANE*blub_pkg::fp_width(src_fmt_i);
        end
    end
endmodule

module test2;
    test i_test ();
endmodule

