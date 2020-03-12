module module_0 #()(
    input clk_i
);
endmodule

// Test
module module_1;
    module_0 mod (.clk_i(1'b1));
endmodule