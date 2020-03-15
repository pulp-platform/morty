module module_1;
    module_2 i_module_2();
    module_external i_external_module();
endmodule

module module_2 #();
endmodule

// Another one bites the dust.
module /* test */ module_3 #()(
    input clk_i
);
endmodule

module module_4 #()(
    clk_i
);

input clk_i;
    module_1 i_module_1();
endmodule : module_4